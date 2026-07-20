mod capture;
pub(crate) mod commands;
pub(crate) mod computer_control;
pub(crate) mod images;
mod llm;
mod memory_markdown;
mod memory_recall;
mod plan;
mod runtime;
mod skills;
pub(crate) mod store;
mod title;
pub(crate) mod tools;
mod user_action;

pub use commands::*;
pub use plan::{PlanApprovalAction, PlanDecision, PlanDecisionResult};
pub use store::AgentStore;
pub use user_action::PendingUserAction;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{hash_map::Entry, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageInputSupport {
    #[default]
    Unknown,
    Supported,
    Unsupported,
}

impl ImageInputSupport {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Supported => 1,
            Self::Unsupported => 2,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Supported,
            2 => Self::Unsupported,
            _ => Self::Unknown,
        }
    }

    pub fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }

    pub fn is_unsupported(self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

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
    pub images: images::ImageService,
    pub computer_control: computer_control::ComputerControlService,
    pub cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_continuations: Mutex<HashMap<String, PendingContinuation>>,
    conversation_lifecycle: Mutex<()>,
    image_capability: AtomicU8,
}

pub struct PendingContinuation {
    pub conversation_id: String,
    pub tool_call_id: String,
    pub state: AgentTurnState,
}

pub struct AgentTurnState {
    pub mode: AgentTurnMode,
    pub messages: Vec<serde_json::Value>,
    pub active_skills: BTreeSet<String>,
}

impl AgentTurnState {
    pub fn new(messages: Vec<serde_json::Value>, mode: AgentTurnMode) -> Self {
        Self {
            mode,
            messages,
            active_skills: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnMode {
    #[default]
    Default,
    ConversationOnly,
    Plan,
}

impl AgentTurnMode {
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::ConversationOnly | Self::Plan)
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
        let images = images::ImageService::new(store.data_dir());
        Self {
            store,
            images,
            computer_control: computer_control::ComputerControlService::new(coordinator),
            cancel_flags: Mutex::new(HashMap::new()),
            pending_continuations: Mutex::new(HashMap::new()),
            conversation_lifecycle: Mutex::new(()),
            image_capability: AtomicU8::new(ImageInputSupport::Unknown.as_u8()),
        }
    }

    pub fn image_capability(&self) -> ImageInputSupport {
        ImageInputSupport::from_u8(self.image_capability.load(Ordering::SeqCst))
    }

    pub fn set_image_capability(
        &self,
        app: &AppHandle,
        capability: ImageInputSupport,
        reason: Option<&str>,
    ) {
        let previous = self.image_capability.swap(capability.as_u8(), Ordering::SeqCst);
        if previous == capability.as_u8() {
            return;
        }
        let _ = app.emit(
            "agent://image-capability",
            json!({
                "supported": capability.is_supported(),
                "unsupported": capability.is_unsupported(),
                "reason": reason,
            }),
        );
    }

    pub fn reset_image_capability(&self, app: &AppHandle) {
        self.set_image_capability(app, ImageInputSupport::Unknown, None);
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
                .store
                .get_pending_plan_approval(conversation_id)?
                .is_some()
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
        self.store.delete_conversation(conversation_id)?;
        self.images.delete_conversation_images(conversation_id)
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

pub const CONVERSATION_ONLY_PROMPT: &str = "\
## Conversation-only mode
Read-only: do not edit the model, run previews/executes, or use computer-operation tools. You may inspect Editor/model state and answer questions. Do not read editing or computer-operation SKILLs. If the user asks for edits, tell them to turn off conversation-only mode.";

pub const PLAN_MODE_PROMPT: &str = "\
## Plan mode
This turn is strictly read-only. Inspect the current Editor and model state with available read-only tools before planning. Do not connect or disconnect Editor, preview or execute edits, change temporary values, write memory, or operate the computer.
Finish by calling submit_plan exactly once with a complete structured plan. Include a concise title and summary, ordered production steps, Mermaid diagram source, acceptance checks, assumptions, and risks. The diagram must contain Mermaid source only: no Markdown fence, HTML, or external links. A plan-mode turn that ends without submit_plan is an error. Plan approval only starts execution; every concrete Cubism edit still requires its normal preview, confirmation, transaction, and verification.";

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
        let mut state = AgentTurnState::new(
            vec![json!({"role": "system", "content": "base"})],
            AgentTurnMode::Plan,
        );
        state.active_skills.insert("parameter-editing".into());

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
        assert_eq!(resumed.mode, AgentTurnMode::Plan);
        assert_eq!(
            resumed.messages.last().unwrap(),
            &json!({
                "role": "tool",
                "tool_call_id": "ask-call",
                "content": "确认",
            })
        );
        assert!(AgentTurnState::new(Vec::new(), AgentTurnMode::Default)
            .active_skills
            .is_empty());
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
        assert!(runtime
            .cancel_flags
            .lock()
            .await
            .contains_key(&conversation_b.id));
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
                    mode: AgentTurnMode::Default,
                    messages: Vec::new(),
                    active_skills: BTreeSet::new(),
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
                state: AgentTurnState::new(Vec::new(), AgentTurnMode::Default),
            },
        );

        assert!(matches!(
            runtime.begin_question_answer("approval").await,
            Err(error) if error.code == "ask_not_found"
        ));
        assert!(runtime.cancel_flags.lock().await.is_empty());
    }
}
