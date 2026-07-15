mod capture;
pub(crate) mod commands;
mod llm;
mod runtime;
pub(crate) mod store;
pub(crate) mod tools;

pub use commands::*;
pub use store::AgentStore;

use serde::Serialize;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
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
}

pub struct PendingContinuation {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub messages: Vec<serde_json::Value>,
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
        }
    }

    pub async fn begin_turn(&self, conversation_id: &str) -> Result<Arc<AtomicBool>, AgentError> {
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

    #[tokio::test]
    async fn only_one_turn_can_own_a_conversation() {
        let runtime = AgentRuntime::default();
        let first = runtime.begin_turn("conversation-a").await.unwrap();

        assert!(matches!(
            runtime.begin_turn("conversation-a").await,
            Err(error) if error.code == "turn_in_progress"
        ));
        assert!(runtime.begin_turn("conversation-b").await.is_ok());

        runtime.finish_turn("conversation-a", &first).await;
        assert!(runtime.begin_turn("conversation-a").await.is_ok());
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
                messages: Vec::new(),
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
}
