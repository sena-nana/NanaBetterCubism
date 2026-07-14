use crate::agent::llm::test_connection;
use crate::agent::runtime::{consolidate_memory, continue_after_ask, run_turn};
use crate::agent::store::{
    ChatMessage, ConversationPlan, ConversationSummary, LlmConfigInput, LlmConfigView,
    MemoryRecord, MemoryUpsertInput, PendingAsk, ProjectRecord,
};
use crate::agent::{AgentError, AgentRuntime};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestResult {
    pub ok: bool,
    pub message: String,
    pub models: Vec<String>,
}

fn runtime(app: &AppHandle) -> Result<Arc<AgentRuntime>, AgentError> {
    app.try_state::<Arc<AgentRuntime>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| AgentError::new("runtime_missing", "Agent 运行时未初始化。"))
}

#[tauri::command]
pub async fn llm_get_config(app: AppHandle) -> Result<LlmConfigView, AgentError> {
    runtime(&app)?.store.get_llm_config_view()
}

#[tauri::command]
pub async fn llm_set_config(
    app: AppHandle,
    input: LlmConfigInput,
) -> Result<LlmConfigView, AgentError> {
    runtime(&app)?.store.set_llm_config(input)
}

#[tauri::command]
pub async fn llm_test_connection(app: AppHandle) -> Result<LlmTestResult, AgentError> {
    let config = runtime(&app)?.store.get_llm_config()?;
    let (ok, message, models) = test_connection(&config).await?;
    Ok(LlmTestResult {
        ok,
        message,
        models,
    })
}

#[tauri::command]
pub async fn agent_list_conversations(app: AppHandle) -> Result<Vec<ConversationSummary>, AgentError> {
    runtime(&app)?.store.list_conversations()
}

#[tauri::command]
pub async fn agent_create_conversation(
    app: AppHandle,
    title: Option<String>,
) -> Result<ConversationSummary, AgentError> {
    let created = runtime(&app)?.store.create_conversation(title)?;
    let _ = app.emit("agent://conversations-changed", json!({}));
    Ok(created)
}

#[tauri::command]
pub async fn agent_get_messages(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<ChatMessage>, AgentError> {
    runtime(&app)?.store.get_messages(&conversation_id)
}

#[tauri::command]
pub async fn agent_send_message(
    app: AppHandle,
    conversation_id: String,
    content: String,
) -> Result<(), AgentError> {
    let text = content.trim().to_string();
    if text.is_empty() {
        return Err(AgentError::new("invalid_message", "消息不能为空。"));
    }
    let runtime = runtime(&app)?;
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_turn(app_clone, runtime_clone, conversation_id, text).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_cancel_turn(app: AppHandle, conversation_id: String) -> Result<(), AgentError> {
    runtime(&app)?.request_cancel(&conversation_id).await;
    Ok(())
}

#[tauri::command]
pub async fn agent_answer_ask(
    app: AppHandle,
    ask_id: String,
    answer: String,
) -> Result<(), AgentError> {
    let text = answer.trim().to_string();
    if text.is_empty() {
        return Err(AgentError::new("invalid_answer", "回答不能为空。"));
    }
    let runtime = runtime(&app)?;
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    tauri::async_runtime::spawn(async move {
        let _ = continue_after_ask(app_clone, runtime_clone, ask_id, text).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_get_plan(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ConversationPlan>, AgentError> {
    runtime(&app)?.store.get_plan(&conversation_id)
}

#[tauri::command]
pub async fn agent_get_pending_ask(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<PendingAsk>, AgentError> {
    runtime(&app)?.store.get_pending_ask(&conversation_id)
}

#[tauri::command]
pub async fn agent_list_projects(app: AppHandle) -> Result<Vec<ProjectRecord>, AgentError> {
    runtime(&app)?.store.list_projects()
}

#[tauri::command]
pub async fn agent_upsert_project(
    app: AppHandle,
    id: Option<String>,
    name: String,
) -> Result<ProjectRecord, AgentError> {
    let project = runtime(&app)?.store.upsert_project(id, name)?;
    let _ = app.emit("agent://conversations-changed", json!({}));
    Ok(project)
}

#[tauri::command]
pub async fn agent_bind_project(
    app: AppHandle,
    conversation_id: String,
    project_id: Option<String>,
) -> Result<(), AgentError> {
    runtime(&app)?
        .store
        .bind_project(&conversation_id, project_id)?;
    let _ = app.emit("agent://conversations-changed", json!({}));
    Ok(())
}

#[tauri::command]
pub async fn memory_list(
    app: AppHandle,
    project_id: Option<String>,
) -> Result<Vec<MemoryRecord>, AgentError> {
    runtime(&app)?.store.list_memories(project_id)
}

#[tauri::command]
pub async fn memory_upsert(
    app: AppHandle,
    input: MemoryUpsertInput,
) -> Result<MemoryRecord, AgentError> {
    runtime(&app)?.store.upsert_memory(input)
}

#[tauri::command]
pub async fn memory_set_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), AgentError> {
    runtime(&app)?.store.set_memory_enabled(&id, enabled)
}

#[tauri::command]
pub async fn agent_consolidate_memory(
    app: AppHandle,
    conversation_id: String,
) -> Result<(), AgentError> {
    let runtime = runtime(&app)?;
    consolidate_memory(app, &runtime, &conversation_id).await
}
