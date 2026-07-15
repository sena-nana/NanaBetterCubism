mod capture;
pub(crate) mod commands;
mod llm;
mod runtime;
mod skills;
pub(crate) mod store;
pub(crate) mod tools;

pub use commands::*;
pub use store::AgentStore;

use serde::Serialize;
use serde_json::json;
use std::collections::{hash_map::Entry, BTreeSet, HashMap};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentError {
    pub code: String,
    pub message: String,
}

impl AgentError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<rusqlite::Error> for AgentError {
    fn from(value: rusqlite::Error) -> Self {
        Self::new("store_error", value.to_string())
    }
}

impl From<reqwest::Error> for AgentError {
    fn from(value: reqwest::Error) -> Self {
        Self::new("llm_http_error", value.to_string())
    }
}

impl From<serde_json::Error> for AgentError {
    fn from(value: serde_json::Error) -> Self {
        Self::new("json_error", value.to_string())
    }
}

#[derive(Default)]
pub struct AgentRuntime {
    pub store: AgentStore,
    pub cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_continuations: Mutex<HashMap<String, PendingContinuation>>,
    conversation_lifecycle: Mutex<()>,
}

pub struct PendingContinuation {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub state: AgentTurnState,
}

pub struct AgentTurnState {
    pub messages: Vec<serde_json::Value>,
    pub active_skills: BTreeSet<String>,
    pub action_steps: usize,
    pub skill_load_steps: usize,
}

impl AgentTurnState {
    pub fn new(messages: Vec<serde_json::Value>) -> Self {
        Self {
            messages,
            active_skills: BTreeSet::new(),
            action_steps: 0,
            skill_load_steps: 0,
        }
    }
}

impl PendingContinuation {
    pub fn resume(mut self, answer: &str) -> AgentTurnState {
        self.state.messages.push(json!({
            "role": "tool",
            "tool_call_id": self.tool_call_id,
            "content": answer,
        }));
        self.state
    }
}

pub(crate) fn emit_conversations_changed(app: &AppHandle) {
    let _ = app.emit("agent://conversations-changed", json!({}));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTurnState {
    CancelRequested,
    PendingCleared,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTurnResult {
    pub state: CancelTurnState,
}

impl AgentRuntime {
    pub fn new(store: AgentStore) -> Self {
        Self {
            store,
            cancel_flags: Mutex::new(HashMap::new()),
            pending_continuations: Mutex::new(HashMap::new()),
            conversation_lifecycle: Mutex::new(()),
        }
    }

    pub async fn begin_turn(&self, conversation_id: &str) -> Result<Arc<AtomicBool>, AgentError> {
        let _lifecycle = self.conversation_lifecycle.lock().await;
        self.store.ensure_active_conversation(conversation_id)?;
        let mut flags = self.cancel_flags.lock().await;
        match flags.entry(conversation_id.to_string()) {
            Entry::Vacant(entry) => {
                let cancel = Arc::new(AtomicBool::new(false));
                entry.insert(cancel.clone());
                Ok(cancel)
            }
            Entry::Occupied(_) => Err(AgentError::new(
                "turn_in_progress",
                "该对话已有回合正在运行。",
            )),
        }
    }

    pub async fn delete_conversation(&self, conversation_id: &str) -> Result<(), AgentError> {
        let _lifecycle = self.conversation_lifecycle.lock().await;
        self.store.ensure_active_conversation(conversation_id)?;
        let running = self.cancel_flags.lock().await.contains_key(conversation_id);
        let awaiting_input = self
            .pending_continuations
            .lock()
            .await
            .values()
            .any(|continuation| continuation.conversation_id == conversation_id)
            || self.store.get_pending_ask(conversation_id)?.is_some();
        if running || awaiting_input {
            return Err(AgentError::new(
                "conversation_busy",
                "对话正在运行或等待回答，暂时无法删除。",
            ));
        }
        self.store.delete_conversation(conversation_id)
    }

    pub async fn begin_answer(
        &self,
        ask_id: &str,
    ) -> Result<(String, Arc<AtomicBool>), AgentError> {
        let conversation_id = self
            .pending_continuations
            .lock()
            .await
            .get(ask_id)
            .map(|continuation| continuation.conversation_id.clone())
            .ok_or_else(|| AgentError::new("ask_not_found", "提问上下文已失效。"))?;
        let cancel = self.begin_turn(&conversation_id).await?;
        Ok((conversation_id, cancel))
    }

    pub async fn finish_turn(&self, conversation_id: &str, cancel: &Arc<AtomicBool>) -> bool {
        let mut flags = self.cancel_flags.lock().await;
        if flags
            .get(conversation_id)
            .is_some_and(|active| Arc::ptr_eq(active, cancel))
        {
            let cancel_requested = cancel.load(std::sync::atomic::Ordering::SeqCst);
            flags.remove(conversation_id);
            cancel_requested
        } else {
            false
        }
    }

    pub async fn clear_pending_ask(&self, conversation_id: &str) -> Result<bool, AgentError> {
        let ask_cleared = self.store.clear_pending_ask(conversation_id)?;
        let mut pending = self.pending_continuations.lock().await;
        let previous_len = pending.len();
        pending.retain(|_, value| value.conversation_id != conversation_id);
        Ok(ask_cleared || pending.len() != previous_len)
    }

    pub async fn request_cancel(
        &self,
        conversation_id: &str,
    ) -> Result<CancelTurnResult, AgentError> {
        if let Some(flag) = self.cancel_flags.lock().await.get(conversation_id) {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(CancelTurnResult {
                state: CancelTurnState::CancelRequested,
            });
        }

        if self.clear_pending_ask(conversation_id).await? {
            Ok(CancelTurnResult {
                state: CancelTurnState::PendingCleared,
            })
        } else {
            Ok(CancelTurnResult {
                state: CancelTurnState::Idle,
            })
        }
    }
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub const SYSTEM_PROMPT: &str = include_str!("prompt.txt");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store::PendingAsk;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    #[test]
    fn cancellation_result_uses_the_typed_frontend_contract() {
        let states = [
            (CancelTurnState::CancelRequested, "cancel_requested"),
            (CancelTurnState::PendingCleared, "pending_cleared"),
            (CancelTurnState::Idle, "idle"),
        ];

        for (state, serialized) in states {
            assert_eq!(
                serde_json::to_value(CancelTurnResult { state }).unwrap(),
                json!({ "state": serialized })
            );
        }
    }

    #[test]
    fn pending_answer_resumes_the_same_turn_skill_state() {
        let mut state = AgentTurnState::new(vec![json!({"role": "system", "content": "base"})]);
        state.active_skills.insert("parameter-editing".into());
        state.action_steps = 3;
        state.skill_load_steps = 1;

        let resumed = PendingContinuation {
            conversation_id: "conversation".into(),
            tool_call_id: "ask-call".into(),
            state,
        }
        .resume("确认");

        assert_eq!(
            resumed.active_skills,
            BTreeSet::from(["parameter-editing".into()])
        );
        assert_eq!(resumed.action_steps, 3);
        assert_eq!(resumed.skill_load_steps, 1);
        assert_eq!(
            resumed.messages.last().unwrap(),
            &json!({
                "role": "tool",
                "tool_call_id": "ask-call",
                "content": "确认",
            })
        );
        assert!(AgentTurnState::new(Vec::new()).active_skills.is_empty());
    }

    #[tokio::test]
    async fn only_one_turn_can_own_a_conversation() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation_a = runtime.store.create_conversation(None).unwrap();
        let conversation_b = runtime.store.create_conversation(None).unwrap();
        let first = runtime.begin_turn(&conversation_a.id).await.unwrap();

        assert!(matches!(
            runtime.begin_turn(&conversation_a.id).await,
            Err(error) if error.code == "turn_in_progress"
        ));
        assert!(runtime.begin_turn(&conversation_b.id).await.is_ok());

        runtime.finish_turn(&conversation_a.id, &first).await;
        assert!(runtime.begin_turn(&conversation_a.id).await.is_ok());
    }

    #[tokio::test]
    async fn cancellation_result_reflects_active_pending_and_idle_state() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation = runtime.store.create_conversation(None).unwrap();

        let active = runtime.begin_turn(&conversation.id).await.unwrap();
        assert_eq!(
            runtime.request_cancel(&conversation.id).await.unwrap(),
            CancelTurnResult {
                state: CancelTurnState::CancelRequested,
            }
        );
        assert!(active.load(Ordering::SeqCst));
        runtime.finish_turn(&conversation.id, &active).await;

        let ask = PendingAsk {
            ask_id: new_id(),
            conversation_id: conversation.id.clone(),
            question: "?".into(),
            options: Vec::new(),
        };
        runtime.store.set_pending_ask(&ask, "tool-call").unwrap();
        runtime.pending_continuations.lock().await.insert(
            ask.ask_id.clone(),
            PendingContinuation {
                conversation_id: conversation.id.clone(),
                tool_call_id: "tool-call".into(),
                state: AgentTurnState {
                    messages: Vec::new(),
                    active_skills: BTreeSet::new(),
                    action_steps: 0,
                    skill_load_steps: 0,
                },
            },
        );

        assert_eq!(
            runtime.request_cancel(&conversation.id).await.unwrap(),
            CancelTurnResult {
                state: CancelTurnState::PendingCleared,
            }
        );
        assert!(runtime
            .store
            .get_pending_ask(&conversation.id)
            .unwrap()
            .is_none());
        assert!(runtime.pending_continuations.lock().await.is_empty());
        assert_eq!(
            runtime.request_cancel(&conversation.id).await.unwrap(),
            CancelTurnResult {
                state: CancelTurnState::Idle,
            }
        );
    }

    #[tokio::test]
    async fn delete_rejects_active_and_pending_conversations() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation = runtime.store.create_conversation(None).unwrap();
        let active = runtime.begin_turn(&conversation.id).await.unwrap();

        assert!(matches!(
            runtime.delete_conversation(&conversation.id).await,
            Err(error) if error.code == "conversation_busy"
        ));
        runtime.finish_turn(&conversation.id, &active).await;

        let ask = PendingAsk {
            ask_id: new_id(),
            conversation_id: conversation.id.clone(),
            question: "?".into(),
            options: Vec::new(),
        };
        runtime.store.set_pending_ask(&ask, "tool-call").unwrap();
        assert!(matches!(
            runtime.delete_conversation(&conversation.id).await,
            Err(error) if error.code == "conversation_busy"
        ));
        runtime.clear_pending_ask(&conversation.id).await.unwrap();

        runtime.delete_conversation(&conversation.id).await.unwrap();
        assert!(matches!(
            runtime.begin_turn(&conversation.id).await,
            Err(error) if error.code == "not_found"
        ));
    }
}
