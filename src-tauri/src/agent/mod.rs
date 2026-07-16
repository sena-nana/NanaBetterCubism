mod capture;
pub(crate) mod commands;
pub(crate) mod computer_control;
mod llm;
mod runtime;
mod skills;
pub(crate) mod store;
pub(crate) mod tools;
mod user_action;

pub use commands::*;
pub use store::AgentStore;
pub use user_action::PendingUserAction;

use serde::Serialize;
use serde_json::{json, Value};
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

pub struct AgentRuntime {
    pub store: AgentStore,
    pub computer_control: computer_control::ComputerControlService,
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
    pub computer_action_steps: usize,
    pub skill_load_steps: usize,
}

impl AgentTurnState {
    pub fn new(messages: Vec<serde_json::Value>) -> Self {
        Self {
            messages,
            active_skills: BTreeSet::new(),
            action_steps: 0,
            computer_action_steps: 0,
            skill_load_steps: 0,
        }
    }
}

impl PendingContinuation {
    pub fn resume(mut self, result: Value) -> AgentTurnState {
        let content = match result {
            Value::String(value) => value,
            value => value.to_string(),
        };
        self.state.messages.push(json!({
            "role": "tool",
            "tool_call_id": self.tool_call_id,
            "content": content,
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
    pub fn new(store: AgentStore, coordinator: crate::service::OperationCoordinator) -> Self {
        Self {
            store,
            computer_control: computer_control::ComputerControlService::new(coordinator),
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
            || self.store.get_pending_question(conversation_id)?.is_some()
            || self
                .computer_control
                .pending_approval_for_conversation(conversation_id)
                .is_some();
        if running || awaiting_input {
            return Err(AgentError::new(
                "conversation_busy",
                "对话正在运行或等待回答，暂时无法删除。",
            ));
        }
        self.store.delete_conversation(conversation_id)
    }

    pub async fn begin_question_answer(
        &self,
        action_id: &str,
    ) -> Result<(String, Arc<AtomicBool>), AgentError> {
        let conversation_id = self
            .pending_continuations
            .lock()
            .await
            .get(action_id)
            .map(|continuation| continuation.conversation_id.clone())
            .ok_or_else(|| AgentError::new("ask_not_found", "提问上下文已失效。"))?;
        if self
            .store
            .get_pending_question(&conversation_id)?
            .is_none_or(|question| question.action_id != action_id)
        {
            return Err(AgentError::new("ask_not_found", "提问上下文已失效。"));
        }
        let cancel = self.begin_turn(&conversation_id).await?;
        Ok((conversation_id, cancel))
    }

    pub async fn begin_user_action(
        &self,
        action_id: &str,
    ) -> Result<(String, Arc<AtomicBool>), AgentError> {
        let conversation_id = self
            .pending_continuations
            .lock()
            .await
            .get(action_id)
            .map(|continuation| continuation.conversation_id.clone())
            .ok_or_else(|| AgentError::new("user_action_not_found", "待处理操作已失效。"))?;
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

    pub async fn clear_pending_user_action(
        &self,
        conversation_id: &str,
    ) -> Result<bool, AgentError> {
        let question_cleared = self.store.clear_pending_user_action(conversation_id)?;
        let approval_cleared = self
            .computer_control
            .pending_approval_for_conversation(conversation_id)
            .is_some();
        self.computer_control.cancel_conversation(conversation_id);
        let mut pending = self.pending_continuations.lock().await;
        let previous_len = pending.len();
        pending.retain(|_, value| value.conversation_id != conversation_id);
        Ok(question_cleared || approval_cleared || pending.len() != previous_len)
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

        if self.clear_pending_user_action(conversation_id).await? {
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

impl Default for AgentRuntime {
    fn default() -> Self {
        Self::new(
            AgentStore::default(),
            crate::service::OperationCoordinator::default(),
        )
    }
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub const SYSTEM_PROMPT: &str = include_str!("prompt.txt");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store::PendingQuestion;
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
        .resume(Value::String("确认".into()));

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
    async fn turns_run_concurrently_across_conversations_and_cancel_independently() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation_a = runtime.store.create_conversation(None, None).unwrap();
        let conversation_b = runtime.store.create_conversation(None, None).unwrap();
        let first = runtime.begin_turn(&conversation_a.id).await.unwrap();

        assert!(matches!(
            runtime.begin_turn(&conversation_a.id).await,
            Err(error) if error.code == "turn_in_progress"
        ));
        let second = runtime.begin_turn(&conversation_b.id).await.unwrap();
        assert_eq!(runtime.cancel_flags.lock().await.len(), 2);

        assert_eq!(
            runtime.request_cancel(&conversation_a.id).await.unwrap(),
            CancelTurnResult {
                state: CancelTurnState::CancelRequested,
            }
        );
        assert!(first.load(Ordering::SeqCst));
        assert!(!second.load(Ordering::SeqCst));

        runtime.finish_turn(&conversation_a.id, &first).await;
        assert!(runtime.cancel_flags.lock().await.contains_key(&conversation_b.id));
        let restarted = runtime.begin_turn(&conversation_a.id).await.unwrap();
        runtime.finish_turn(&conversation_b.id, &second).await;
        runtime.finish_turn(&conversation_a.id, &restarted).await;
        assert!(runtime.cancel_flags.lock().await.is_empty());
    }

    #[tokio::test]
    async fn cancellation_result_reflects_active_pending_and_idle_state() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation = runtime.store.create_conversation(None, None).unwrap();

        let active = runtime.begin_turn(&conversation.id).await.unwrap();
        assert_eq!(
            runtime.request_cancel(&conversation.id).await.unwrap(),
            CancelTurnResult {
                state: CancelTurnState::CancelRequested,
            }
        );
        assert!(active.load(Ordering::SeqCst));
        runtime.finish_turn(&conversation.id, &active).await;

        let question = PendingQuestion {
            action_id: new_id(),
            conversation_id: conversation.id.clone(),
            question: "?".into(),
            options: Vec::new(),
        };
        runtime
            .store
            .set_pending_question(&question, "tool-call")
            .unwrap();
        runtime.pending_continuations.lock().await.insert(
            question.action_id.clone(),
            PendingContinuation {
                conversation_id: conversation.id.clone(),
                tool_call_id: "tool-call".into(),
                state: AgentTurnState {
                    messages: Vec::new(),
                    active_skills: BTreeSet::new(),
                    action_steps: 0,
                    computer_action_steps: 0,
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
            .get_pending_question(&conversation.id)
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
        let conversation = runtime.store.create_conversation(None, None).unwrap();
        let active = runtime.begin_turn(&conversation.id).await.unwrap();

        assert!(matches!(
            runtime.delete_conversation(&conversation.id).await,
            Err(error) if error.code == "conversation_busy"
        ));
        runtime.finish_turn(&conversation.id, &active).await;

        let question = PendingQuestion {
            action_id: new_id(),
            conversation_id: conversation.id.clone(),
            question: "?".into(),
            options: Vec::new(),
        };
        runtime
            .store
            .set_pending_question(&question, "tool-call")
            .unwrap();
        assert!(matches!(
            runtime.delete_conversation(&conversation.id).await,
            Err(error) if error.code == "conversation_busy"
        ));
        runtime
            .clear_pending_user_action(&conversation.id)
            .await
            .unwrap();

        runtime.delete_conversation(&conversation.id).await.unwrap();
        assert!(matches!(
            runtime.begin_turn(&conversation.id).await,
            Err(error) if error.code == "not_found"
        ));
    }

    #[tokio::test]
    async fn free_text_answers_cannot_resume_a_structured_approval() {
        let runtime = AgentRuntime::default();
        runtime.store.open(":memory:".into()).unwrap();
        let conversation = runtime.store.create_conversation(None, None).unwrap();
        runtime.pending_continuations.lock().await.insert(
            "approval".into(),
            PendingContinuation {
                conversation_id: conversation.id,
                tool_call_id: "computer-call".into(),
                state: AgentTurnState::new(Vec::new()),
            },
        );

        assert!(matches!(
            runtime.begin_question_answer("approval").await,
            Err(error) if error.code == "ask_not_found"
        ));
        assert!(runtime.cancel_flags.lock().await.is_empty());
    }
}
