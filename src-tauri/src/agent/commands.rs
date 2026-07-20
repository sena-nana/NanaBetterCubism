use crate::agent::computer_control::ComputerOperationStatus;
use crate::agent::images::{ChatImageAttachment, ImagePrepareInput, ImagePrepareResult};
use crate::agent::llm::test_connection;
use crate::agent::runtime::{continue_after_computer_approval, continue_after_question, run_turn};
use crate::agent::store::{
    ChatMessage, ConversationPlan, ConversationSummary, LlmConfigInput, LlmConfigView,
    MemoryViewRecord, ProjectRecord,
};
use crate::agent::title::generate_conversation_title;
use crate::agent::tools::tool_display_name;
use crate::agent::{
    emit_conversations_changed, AgentError, AgentRuntime, AgentTurnMode, CancelTurnResult,
    PendingUserAction, PlanDecision, PlanDecisionResult,
};
use crate::service::{official_api, EditorService};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestResult {
    pub ok: bool,
    pub message: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageView {
    id: String,
    role: String,
    content: String,
    tool_name: Option<String>,
    tool_display_name: Option<String>,
    tool_status: Option<String>,
    attachments: Vec<ChatImageAttachment>,
    created_at: String,
}

impl From<ChatMessage> for ChatMessageView {
    fn from(message: ChatMessage) -> Self {
        let tool_display_name = message
            .tool_name
            .as_deref()
            .and_then(tool_display_name)
            .map(str::to_string);
        Self {
            id: message.id,
            role: message.role,
            content: message.content,
            tool_name: message.tool_name,
            tool_display_name,
            tool_status: message.tool_status,
            attachments: message.attachments,
            created_at: message.created_at,
        }
    }
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
pub async fn agent_list_conversations(
    app: AppHandle,
) -> Result<Vec<ConversationSummary>, AgentError> {
    runtime(&app)?.store.list_conversations()
}

#[tauri::command]
pub async fn agent_create_conversation(
    app: AppHandle,
    title: Option<String>,
) -> Result<ConversationSummary, AgentError> {
    let editor = app.state::<EditorService>();
    let document = official_api::current_modeling_document(editor.inner()).await;
    let created = runtime(&app)?.store.create_conversation(
        title,
        document
            .as_ref()
            .map(|value| (value.document_key.as_str(), value.document_path.as_str())),
    )?;
    emit_conversations_changed(&app);
    Ok(created)
}

#[tauri::command]
pub async fn agent_set_conversation_pinned(
    app: AppHandle,
    conversation_id: String,
    pinned: bool,
) -> Result<bool, AgentError> {
    let pinned = runtime(&app)?
        .store
        .set_conversation_pinned(&conversation_id, pinned)?;
    emit_conversations_changed(&app);
    Ok(pinned)
}

#[tauri::command]
pub async fn agent_delete_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<(), AgentError> {
    runtime(&app)?.delete_conversation(&conversation_id).await?;
    emit_conversations_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn agent_get_messages(
    app: AppHandle,
    conversation_id: String,
) -> Result<Vec<ChatMessageView>, AgentError> {
    let runtime = runtime(&app)?;
    runtime.store.ensure_active_conversation(&conversation_id)?;
    runtime
        .store
        .get_messages(&conversation_id)
        .map(|messages| messages.into_iter().map(ChatMessageView::from).collect())
}

#[tauri::command]
pub async fn agent_prepare_images(
    app: AppHandle,
    inputs: Vec<ImagePrepareInput>,
    remaining_slots: usize,
) -> Result<ImagePrepareResult, AgentError> {
    Ok(runtime(&app)?.images.prepare(inputs, remaining_slots))
}

#[tauri::command]
pub async fn agent_discard_image_drafts(
    app: AppHandle,
    draft_ids: Vec<String>,
) -> Result<(), AgentError> {
    runtime(&app)?.images.discard(&draft_ids)
}

#[tauri::command]
pub async fn agent_send_message(
    app: AppHandle,
    conversation_id: String,
    content: String,
    image_draft_ids: Vec<String>,
    mode: Option<AgentTurnMode>,
) -> Result<ChatMessageView, AgentError> {
    let text = content.trim().to_string();
    if text.is_empty() && image_draft_ids.is_empty() {
        return Err(AgentError::new("invalid_message", "消息不能为空。"));
    }
    let runtime = runtime(&app)?;
    let cancel = runtime.begin_turn(&conversation_id).await?;
    let commit = match runtime.images.commit(&conversation_id, &image_draft_ids) {
        Ok(commit) => commit,
        Err(error) => {
            runtime.finish_turn(&conversation_id, &cancel).await;
            return Err(error);
        }
    };
    let message = match runtime.store.append_message_with_attachments(
        &conversation_id,
        "user",
        &text,
        None,
        None,
        &commit.attachments,
    ) {
        Ok(message) => message,
        Err(error) => {
            runtime.images.rollback(commit);
            runtime.finish_turn(&conversation_id, &cancel).await;
            return Err(error);
        }
    };
    let title_seed = if text.is_empty() {
        format!(
            "查看图片：{}",
            message
                .attachments
                .first()
                .map(|attachment| attachment.name.as_str())
                .unwrap_or("图片")
        )
    } else {
        text.clone()
    };
    let title: String = title_seed.chars().take(24).collect();
    let renamed = runtime
        .store
        .set_conversation_title_if_default(&conversation_id, &title)
        .unwrap_or(false);
    emit_conversations_changed(&app);
    if renamed {
        tauri::async_runtime::spawn(generate_conversation_title(
            app.clone(),
            runtime.clone(),
            conversation_id.clone(),
            title_seed,
        ));
    }
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    let mode = mode.unwrap_or_default();
    tauri::async_runtime::spawn(async move {
        let _ = run_turn(
            app_clone,
            runtime_clone,
            conversation_id,
            mode,
            None,
            cancel,
        )
        .await;
    });
    Ok(message.into())
}

#[tauri::command]
pub async fn agent_decide_plan(
    app: AppHandle,
    action_id: String,
    decision: PlanDecision,
    revision: Option<String>,
) -> Result<PlanDecisionResult, AgentError> {
    let runtime = runtime(&app)?;
    let pending = runtime
        .store
        .get_pending_plan_approval_by_action(&action_id)?
        .ok_or_else(|| AgentError::new("plan_not_found", "待确认计划已失效。"))?;
    let conversation_id = pending.action.conversation_id.clone();

    if decision == PlanDecision::Cancel {
        let pending = runtime
            .store
            .take_pending_plan_approval(&action_id)?
            .ok_or_else(|| AgentError::new("plan_not_found", "待确认计划已失效。"))?;
        let plan = runtime
            .store
            .upsert_plan(&conversation_id, pending.plan.todo_steps("cancelled"))?;
        let _ = app.emit(
            "agent://plan",
            serde_json::json!({"conversationId": conversation_id, "plan": plan}),
        );
        emit_conversations_changed(&app);
        return Ok(PlanDecisionResult::Cancelled);
    }

    let revision = revision.unwrap_or_default().trim().to_string();
    if decision == PlanDecision::Revise && revision.is_empty() {
        return Err(AgentError::new("invalid_revision", "修改要求不能为空。"));
    }

    let cancel = runtime.begin_turn(&conversation_id).await?;
    let pending = match runtime.store.take_pending_plan_approval(&action_id) {
        Ok(Some(pending)) => pending,
        Ok(None) => {
            runtime.finish_turn(&conversation_id, &cancel).await;
            return Err(AgentError::new("plan_not_found", "待确认计划已失效。"));
        }
        Err(error) => {
            runtime.finish_turn(&conversation_id, &cancel).await;
            return Err(error);
        }
    };

    let (mode, prompt, user_message, result) = match decision {
        PlanDecision::Approve => (
            AgentTurnMode::Default,
            format!(
                "The user approved the following exact plan. Start executing it now in default mode. Plan approval does not authorize any concrete Cubism edit or computer operation; continue to use every required preview, user confirmation, transaction, cancellation, and reread verification.\n\n{}",
                pending.plan.markdown()
            ),
            "已确认计划，开始执行。".to_string(),
            PlanDecisionResult::ExecutionStarted,
        ),
        PlanDecision::Revise => (
            AgentTurnMode::Plan,
            format!(
                "Revise the complete plan in read-only plan mode. Keep useful verified facts, apply the latest revision request, inspect read-only state again when needed, and submit a full replacement through submit_plan.\n\nLatest revision request:\n{}\n\nPrevious plan:\n{}",
                revision,
                pending.plan.markdown()
            ),
            format!("计划修改要求：{revision}"),
            PlanDecisionResult::RevisionStarted,
        ),
        PlanDecision::Cancel => unreachable!(),
    };
    if let Err(error) =
        runtime
            .store
            .append_message(&conversation_id, "user", &user_message, None, None)
    {
        let _ = runtime.store.set_pending_plan_approval(&pending);
        runtime.finish_turn(&conversation_id, &cancel).await;
        return Err(error);
    }
    emit_conversations_changed(&app);
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run_turn(
            app_clone,
            runtime_clone,
            conversation_id,
            mode,
            Some(prompt),
            cancel,
        )
        .await;
    });
    Ok(result)
}

#[tauri::command]
pub async fn agent_cancel_turn(
    app: AppHandle,
    conversation_id: String,
) -> Result<CancelTurnResult, AgentError> {
    let runtime = runtime(&app)?;
    let awaiting_computer_approval = runtime
        .computer_control
        .pending_approval_for_conversation(&conversation_id)
        .is_some();
    let result = runtime.request_cancel(&conversation_id).await?;
    if awaiting_computer_approval {
        let _ = app.emit(
            "agent://computer-operation",
            serde_json::json!({
                "conversationId": conversation_id,
                "status": ComputerOperationStatus::Cancelled,
            }),
        );
    }
    Ok(result)
}

async fn answer_question(
    app: AppHandle,
    action_id: String,
    answer: String,
) -> Result<(), AgentError> {
    let text = answer.trim().to_string();
    if text.is_empty() {
        return Err(AgentError::new("invalid_answer", "回答不能为空。"));
    }
    let runtime = runtime(&app)?;
    let (conversation_id, cancel) = runtime.begin_question_answer(&action_id).await?;
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    tauri::async_runtime::spawn(async move {
        let _ = continue_after_question(
            app_clone,
            runtime_clone,
            action_id,
            conversation_id,
            text,
            cancel,
        )
        .await;
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_answer_question(
    app: AppHandle,
    action_id: String,
    answer: String,
) -> Result<(), AgentError> {
    answer_question(app, action_id, answer).await
}

#[tauri::command]
pub async fn agent_decide_computer_operation(
    app: AppHandle,
    action_id: String,
    approved: bool,
) -> Result<(), AgentError> {
    let runtime = runtime(&app)?;
    let approval = runtime
        .computer_control
        .pending_approval(&action_id)
        .ok_or_else(|| AgentError::new("approval_not_found", "电脑代理授权请求已失效。"))?;
    let (conversation_id, cancel) = runtime.begin_user_action(&action_id).await?;
    if approval.conversation_id != conversation_id {
        runtime.finish_turn(&conversation_id, &cancel).await;
        return Err(AgentError::new(
            "approval_not_found",
            "电脑代理授权请求不属于当前对话。",
        ));
    }
    let app_clone = app.clone();
    let runtime_clone = runtime.clone();
    tauri::async_runtime::spawn(async move {
        let _ = continue_after_computer_approval(
            app_clone,
            runtime_clone,
            action_id,
            conversation_id,
            approved,
            cancel,
        )
        .await;
    });
    Ok(())
}

#[tauri::command]
pub async fn agent_get_plan(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<ConversationPlan>, AgentError> {
    let runtime = runtime(&app)?;
    runtime.store.ensure_active_conversation(&conversation_id)?;
    runtime.store.get_plan(&conversation_id)
}

#[tauri::command]
pub async fn agent_get_pending_user_action(
    app: AppHandle,
    conversation_id: String,
) -> Result<Option<PendingUserAction>, AgentError> {
    let runtime = runtime(&app)?;
    runtime.store.ensure_active_conversation(&conversation_id)?;
    if let Some(approval) = runtime
        .computer_control
        .pending_approval_for_conversation(&conversation_id)
    {
        return Ok(Some(approval.into()));
    }
    if let Some(approval) = runtime.store.get_pending_plan_approval(&conversation_id)? {
        return Ok(Some(approval.action.into()));
    }
    Ok(runtime
        .store
        .get_pending_question(&conversation_id)?
        .map(PendingUserAction::from))
}

#[tauri::command]
pub async fn agent_list_projects(app: AppHandle) -> Result<Vec<ProjectRecord>, AgentError> {
    runtime(&app)?.store.list_projects()
}

#[tauri::command]
pub async fn memory_list(
    app: AppHandle,
    scope: String,
    project_id: Option<String>,
) -> Result<Vec<MemoryViewRecord>, AgentError> {
    runtime(&app)?.store.list_memory_views(&scope, project_id)
}

#[tauri::command]
pub async fn memory_set_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), AgentError> {
    runtime(&app)?.store.set_memory_enabled(&id, enabled)
}
