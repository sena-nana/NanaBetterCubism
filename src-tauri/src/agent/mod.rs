mod capture;
pub(crate) mod commands;
mod llm;
mod runtime;
pub(crate) mod store;
pub(crate) mod tools;

pub use commands::*;
pub use store::AgentStore;

use serde::Serialize;
use std::collections::HashMap;
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

impl AgentRuntime {
    pub fn new(store: AgentStore) -> Self {
        Self {
            store,
            cancel_flags: Mutex::new(HashMap::new()),
            pending_continuations: Mutex::new(HashMap::new()),
        }
    }

    pub async fn request_cancel(&self, conversation_id: &str) {
        if let Some(flag) = self.cancel_flags.lock().await.get(conversation_id) {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let mut pending = self.pending_continuations.lock().await;
        pending.retain(|_, value| value.conversation_id != conversation_id);
        let _ = self.store.clear_pending_ask(conversation_id);
    }
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub const SYSTEM_PROMPT: &str = include_str!("prompt.txt");
