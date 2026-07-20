use crate::agent::computer_control::ComputerOperationStatus;
use crate::agent::llm::{
    chat_completions_stream, content_to_text, image_file_to_data_url, ToolCallPayload,
};
use crate::agent::plan::{PendingPlanApproval, PlanApprovalAction};
use crate::agent::skills::{self, READ_SKILL_TOOL_NAME};
use crate::agent::tools::{
    advertised_tool_names, emit_tool, execute_tool, tool_definitions, ToolExecutionContext,
    ToolOutcome,
};
use crate::agent::{
    emit_conversations_changed, new_id, AgentError, AgentRuntime, AgentTurnMode, AgentTurnState,
    ImageInputSupport, PendingContinuation, PendingUserAction, CONVERSATION_ONLY_PROMPT,
    PLAN_MODE_PROMPT, SYSTEM_PROMPT,
};
use crate::service::EditorService;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub async fn run_turn(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    conversation_id: String,
    mode: AgentTurnMode,
    additional_prompt: Option<String>,
    cancel: Arc<AtomicBool>,
) -> Result<(), AgentError> {
    let editor = app.state::<EditorService>();
    let result = run_turn_inner(
        &app,
        &runtime,
        editor.inner(),
        &conversation_id,
        None,
        mode,
        additional_prompt,
        cancel.clone(),
    )
    .await;

    let result = finalize_turn(&app, &runtime, &conversation_id, &cancel, result).await;
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

pub async fn continue_after_question(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    action_id: String,
    conversation_id: String,
    answer: String,
    cancel: Arc<AtomicBool>,
) -> Result<(), AgentError> {
    let result = async {
        let (question, _tool_call_id) = runtime
            .store
            .take_pending_question(&action_id)?
            .ok_or_else(|| AgentError::new("ask_not_found", "没有等待中的提问。"))?;
        if question.conversation_id != conversation_id {
            return Err(AgentError::new("ask_not_found", "提问上下文已失效。"));
        }

        let continuation = runtime
            .pending_continuations
            .lock()
            .await
            .remove(&question.action_id)
            .ok_or_else(|| AgentError::new("ask_not_found", "提问上下文已失效。"))?;

        let state = continuation.resume(Value::String(answer.clone()));
        let mode = state.mode;
        runtime.store.append_message(
            &question.conversation_id,
            "user",
            &format!("回答：{answer}"),
            None,
            None,
        )?;
        emit_conversations_changed(&app);

        let editor = app.state::<EditorService>();
        run_turn_inner(
            &app,
            &runtime,
            editor.inner(),
            &conversation_id,
            Some(state),
            mode,
            None,
            cancel.clone(),
        )
        .await
    }
    .await;

    let result = finalize_turn(&app, &runtime, &conversation_id, &cancel, result).await;
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

pub async fn continue_after_computer_approval(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    action_id: String,
    conversation_id: String,
    approved: bool,
    cancel: Arc<AtomicBool>,
) -> Result<(), AgentError> {
    let result = async {
        let approval = runtime
            .computer_control
            .pending_approval(&action_id)
            .filter(|approval| approval.conversation_id == conversation_id)
            .ok_or_else(|| AgentError::new("approval_not_found", "电脑代理授权请求已失效。"))?;
        let decision = runtime.computer_control.decide(&action_id, approved)?;
        let continuation = runtime
            .pending_continuations
            .lock()
            .await
            .remove(&action_id)
            .ok_or_else(|| AgentError::new("approval_not_found", "授权上下文已失效。"))?;
        let tool_call_id = continuation.tool_call_id.clone();
        let state = continuation.resume(decision);
        let mode = state.mode;
        runtime.store.append_tool_trace(
            &approval.conversation_id,
            &tool_call_id,
            "request_computer_operation",
            if approved {
                r#"{"decision":"approved"}"#
            } else {
                r#"{"decision":"rejected"}"#
            },
            if approved {
                r#"{"grantCreated":true}"#
            } else {
                r#"{"grantCreated":false}"#
            },
            if approved { "approved" } else { "rejected" },
        )?;
        runtime.store.append_message(
            &approval.conversation_id,
            "user",
            if approved {
                "已授权本次电脑代理操作。"
            } else {
                "已拒绝本次电脑代理操作。"
            },
            None,
            None,
        )?;
        let _ = app.emit(
            "agent://computer-operation",
            json!({
                "conversationId": conversation_id,
                "status": if approved {
                    ComputerOperationStatus::Authorized
                } else {
                    ComputerOperationStatus::Cancelled
                }
            }),
        );
        emit_conversations_changed(&app);

        let editor = app.state::<EditorService>();
        run_turn_inner(
            &app,
            &runtime,
            editor.inner(),
            &conversation_id,
            Some(state),
            mode,
            None,
            cancel.clone(),
        )
        .await
    }
    .await;

    if approved && result.is_err() && !cancel.load(Ordering::SeqCst) {
        let _ = app.emit(
            "agent://computer-operation",
            json!({
                "conversationId": conversation_id,
                "status": ComputerOperationStatus::Failed,
            }),
        );
    }
    let result = finalize_turn(&app, &runtime, &conversation_id, &cancel, result).await;
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

enum TurnEnd {
    Finished,
    WaitingUser,
}

async fn finalize_turn(
    app: &AppHandle,
    runtime: &AgentRuntime,
    conversation_id: &str,
    cancel: &Arc<AtomicBool>,
    result: Result<TurnEnd, AgentError>,
) -> Result<TurnEnd, AgentError> {
    let cancelled = runtime.finish_turn(conversation_id, cancel).await;
    if cancelled {
        let had_computer_operation = runtime.computer_control.has_active_grant(conversation_id)
            || runtime
                .computer_control
                .pending_approval_for_conversation(conversation_id)
                .is_some();
        runtime.clear_pending_user_action(conversation_id).await?;
        if had_computer_operation {
            let _ = app.emit(
                "agent://computer-operation",
                json!({
                    "conversationId": conversation_id,
                    "status": ComputerOperationStatus::Cancelled,
                }),
            );
        }
        return Err(AgentError::new("cancelled", "已取消。"));
    }
    if result.is_err() {
        let had_grant = runtime.computer_control.has_active_grant(conversation_id);
        let _ = runtime.clear_pending_user_action(conversation_id).await;
        if had_grant {
            let _ = app.emit(
                "agent://computer-operation",
                json!({
                    "conversationId": conversation_id,
                    "status": ComputerOperationStatus::Failed,
                }),
            );
        }
        return result;
    }
    if matches!(&result, Ok(TurnEnd::Finished)) {
        runtime
            .computer_control
            .revoke_grant_for_conversation(conversation_id);
    }
    result
}

fn emit_finished(app: &AppHandle, conversation_id: &str, result: &Result<TurnEnd, AgentError>) {
    match result {
        Ok(TurnEnd::Finished) => {
            let _ = app.emit(
                "agent://turn-finished",
                json!({
                    "conversationId": conversation_id,
                    "ok": true,
                    "message": "完成"
                }),
            );
            emit_conversations_changed(app);
        }
        Ok(TurnEnd::WaitingUser) => {
            emit_conversations_changed(app);
        }
        Err(error) => {
            let _ = app.emit(
                "agent://turn-finished",
                json!({
                    "conversationId": conversation_id,
                    "ok": false,
                    "message": error.message
                }),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_turn_inner(
    app: &AppHandle,
    runtime: &Arc<AgentRuntime>,
    editor: &EditorService,
    conversation_id: &str,
    existing_state: Option<AgentTurnState>,
    mode: AgentTurnMode,
    additional_prompt: Option<String>,
    cancel: Arc<AtomicBool>,
) -> Result<TurnEnd, AgentError> {
    let config = runtime.store.get_llm_config()?;
    let mut image_stripped_once = false;
    let mut state = if let Some(existing) = existing_state {
        existing
    } else {
        let mut seeded = vec![
            json!({
                "role": "system",
                "content": SYSTEM_PROMPT
            }),
            json!({
                "role": "system",
                "content": skills::catalog_prompt()?
            }),
        ];
        if mode == AgentTurnMode::ConversationOnly {
            seeded.push(json!({
                "role": "system",
                "content": CONVERSATION_ONLY_PROMPT
            }));
        } else if mode == AgentTurnMode::Plan {
            seeded.push(json!({
                "role": "system",
                "content": PLAN_MODE_PROMPT
            }));
        }
        if let Some(prompt) = additional_prompt {
            seeded.push(json!({
                "role": "system",
                "content": prompt
            }));
        }
        for item in runtime.store.get_messages(conversation_id)? {
            if item.role == "tool" {
                continue;
            }
            seeded.push(json!({
                "role": item.role,
                "content": message_content(&item),
            }));
        }
        AgentTurnState::new(seeded, mode)
    };

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(AgentError::new("cancelled", "已取消。"));
        }

        let image_supported = runtime.image_capability() != ImageInputSupport::Unsupported;
        let tools = tool_definitions(&state.active_skills, state.mode, image_supported)?;
        let advertised = advertised_tool_names(&tools);
        let app_for_capability = app.clone();
        let assistant = {
            let conversation_id = conversation_id.to_string();
            let app = app.clone();
            match chat_completions_stream(&config, &state.messages, &tools, move |piece| {
                let _ = app.emit(
                    "agent://turn-delta",
                    json!({ "conversationId": conversation_id, "text": piece }),
                );
            })
            .await
            {
                Ok(message) => message,
                Err(error) if error.code == "llm_image_unsupported" && !image_stripped_once => {
                    image_stripped_once = true;
                    runtime.set_image_capability(
                        &app_for_capability,
                        ImageInputSupport::Unsupported,
                        Some("model reported image input is not supported"),
                    );
                    state.messages.push(json!({
                        "role": "system",
                        "content": "系统通知：当前模型不支持图片输入。从现在起禁止调用 capture_cubism_editor_window（查看 Editor 窗口），不要再请求或引用图片；如用户需要查看窗口或图片，请提示前往「设置」更换支持视觉的模型。已有图片内容已转为文本占位，请基于文本继续。"
                    }));
                    strip_image_content(&mut state.messages);
                    continue;
                }
                Err(error) => return Err(error),
            }
        };
        let text = content_to_text(&assistant.content);
        if !text.is_empty() {
            let _ = runtime
                .store
                .append_message(conversation_id, "assistant", &text, None, None);
        }

        let tool_calls = assistant.tool_calls.clone().unwrap_or_default();
        if tool_calls.is_empty() {
            if state.mode == AgentTurnMode::Plan {
                return Err(AgentError::new(
                    "plan_not_submitted",
                    "规划回合必须通过 submit_plan 提交完整计划。",
                ));
            }
            if runtime.computer_control.has_active_grant(conversation_id) {
                runtime
                    .computer_control
                    .revoke_grant_for_conversation(conversation_id);
                return Err(AgentError::new(
                    "computer_operation_unfinished",
                    "电脑代理操作未提交真实结果，授权已失效。",
                ));
            }
            return Ok(TurnEnd::Finished);
        }

        let includes_skill_load = validate_tool_call_batch(&tool_calls, &advertised)?;

        state.messages.push(json!({
            "role": "assistant",
            "content": assistant.content.clone().unwrap_or(Value::Null),
            "tool_calls": tool_calls.iter().map(|call| json!({
                "id": call.id,
                "type": call.r#type.clone().unwrap_or_else(|| "function".into()),
                "function": {
                    "name": call.function.name,
                    "arguments": call.function.arguments,
                }
            })).collect::<Vec<_>>(),
        }));

        if includes_skill_load {
            let skill_calls = skill_load_calls(&tool_calls);
            for call in &skill_calls {
                emit_tool(
                    app,
                    conversation_id,
                    &call.id,
                    READ_SKILL_TOOL_NAME,
                    "started",
                    "",
                );
            }
            let loaded = match load_skills(&mut state.active_skills, &skill_calls) {
                Ok(loaded) => loaded,
                Err(error) => {
                    for call in &skill_calls {
                        emit_tool(
                            app,
                            conversation_id,
                            &call.id,
                            READ_SKILL_TOOL_NAME,
                            "failed",
                            &error.message,
                        );
                        let _ = runtime.store.append_message(
                            conversation_id,
                            "tool",
                            &error.message,
                            Some(READ_SKILL_TOOL_NAME),
                            Some("failed"),
                        );
                        let _ = runtime.store.append_tool_trace(
                            conversation_id,
                            &call.id,
                            READ_SKILL_TOOL_NAME,
                            &call.function.arguments,
                            &error.code,
                            "failed",
                        );
                    }
                    return Err(error);
                }
            };
            for (call, (tool_call_id, content)) in skill_calls.iter().zip(loaded) {
                emit_tool(
                    app,
                    conversation_id,
                    &tool_call_id,
                    READ_SKILL_TOOL_NAME,
                    "finished",
                    "已读取任务技能",
                );
                let _ = runtime.store.append_message(
                    conversation_id,
                    "tool",
                    "已读取任务技能",
                    Some(READ_SKILL_TOOL_NAME),
                    Some("finished"),
                );
                let _ = runtime.store.append_tool_trace(
                    conversation_id,
                    &tool_call_id,
                    READ_SKILL_TOOL_NAME,
                    &call.function.arguments,
                    "skill_loaded",
                    "finished",
                );
                state.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                }));
            }
        }
        if tool_calls
            .iter()
            .all(|call| call.function.name == READ_SKILL_TOOL_NAME)
        {
            continue;
        }

        for call in tool_calls {
            if call.function.name == READ_SKILL_TOOL_NAME {
                continue;
            }
            if cancel.load(Ordering::SeqCst) {
                return Err(AgentError::new("cancelled", "已取消。"));
            }
            let outcome = execute_tool(
                ToolExecutionContext {
                    app,
                    runtime,
                    editor,
                    conversation_id,
                    tool_call_id: &call.id,
                    cancel: cancel.clone(),
                    mode: state.mode,
                },
                &call.function.name,
                &call.function.arguments,
            )
            .await;
            let outcome = match outcome {
                Ok(outcome) => outcome,
                Err(error) if error.code == "cancelled" => return Err(error),
                Err(error) => {
                    state.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": tool_error_content(&error),
                    }));
                    continue;
                }
            };

            match outcome {
                ToolOutcome::AwaitUser {
                    action,
                    tool_call_id,
                } => {
                    runtime.pending_continuations.lock().await.insert(
                        action.action_id().to_string(),
                        PendingContinuation {
                            conversation_id: conversation_id.into(),
                            tool_call_id,
                            state,
                        },
                    );
                    let _ = app.emit(
                        "agent://user-action",
                        json!({ "conversationId": conversation_id, "action": action }),
                    );
                    return Ok(TurnEnd::WaitingUser);
                }
                ToolOutcome::PlanSubmitted(plan) => {
                    let markdown = plan.markdown();
                    runtime.store.append_message(
                        conversation_id,
                        "assistant",
                        &markdown,
                        None,
                        None,
                    )?;
                    let todo = runtime
                        .store
                        .upsert_plan(conversation_id, plan.todo_steps("pending"))?;
                    let approval = PendingPlanApproval {
                        action: PlanApprovalAction {
                            action_id: new_id(),
                            conversation_id: conversation_id.into(),
                            title: plan.title.clone(),
                        },
                        plan,
                    };
                    runtime.store.set_pending_plan_approval(&approval)?;
                    let _ = app.emit(
                        "agent://turn-delta",
                        json!({ "conversationId": conversation_id, "text": markdown }),
                    );
                    let _ = app.emit(
                        "agent://plan",
                        json!({ "conversationId": conversation_id, "plan": todo }),
                    );
                    let action = PendingUserAction::from(approval.action);
                    let _ = app.emit(
                        "agent://user-action",
                        json!({ "conversationId": conversation_id, "action": action }),
                    );
                    emit_conversations_changed(app);
                    return Ok(TurnEnd::WaitingUser);
                }
                ToolOutcome::Result {
                    content,
                    image_path,
                } => {
                    state.messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": content,
                    }));
                    if let Some(path) = image_path {
                        if runtime.image_capability() == ImageInputSupport::Unsupported {
                            state.messages.push(json!({
                                "role": "user",
                                "content": "已截取 Cubism Editor 窗口，但当前模型不支持图片输入，无法查看该图像。请基于工具返回的文本结果继续，并提示用户更换支持视觉的模型。"
                            }));
                            continue;
                        }
                        match image_file_to_data_url(&path) {
                            Ok(data_url) => {
                                state.messages.push(json!({
                                    "role": "user",
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": "以下是刚才截屏得到的 Cubism Editor 窗口图像，请结合工具返回继续分析。"
                                        },
                                        {
                                            "type": "image_url",
                                            "image_url": { "url": data_url }
                                        }
                                    ]
                                }));
                            }
                            Err(error) => {
                                state.messages.push(json!({
                                    "role": "user",
                                    "content": format!("截屏文件无法作为图像注入：{}", error.message),
                                }));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn message_content(message: &crate::agent::store::ChatMessage) -> Value {
    if message.attachments.is_empty() {
        return Value::String(message.content.clone());
    }
    let mut parts = Vec::new();
    if !message.content.trim().is_empty() {
        parts.push(json!({ "type": "text", "text": message.content }));
    }
    for attachment in &message.attachments {
        if !attachment.available {
            parts.push(json!({
                "type": "text",
                "text": format!("[图片不可用：{}]", attachment.name),
            }));
            continue;
        }
        match image_file_to_data_url(&attachment.path) {
            Ok(data_url) => parts.push(json!({
                "type": "image_url",
                "image_url": { "url": data_url },
            })),
            Err(_) => parts.push(json!({
                "type": "text",
                "text": format!("[图片不可用：{}]", attachment.name),
            })),
        }
    }
    Value::Array(parts)
}

fn tool_error_content(error: &AgentError) -> String {
    json!({
        "ok": false,
        "error": {
            "code": error.code,
            "message": error.message,
        }
    })
    .to_string()
}

/// 将消息序列中的 `image_url` 多模态片段替换为文本占位，纯图消息退化为文本。
/// 用于模型不支持图片输入时剥离图片内容，避免反复触发同一错误。
pub fn strip_image_content(messages: &mut [Value]) {
    for message in messages.iter_mut() {
        let Some(content) = message.get_mut("content") else {
            continue;
        };
        let Value::Array(parts) = content else {
            continue;
        };
        let mut had_image = false;
        for part in parts.iter_mut() {
            if part.get("type").and_then(Value::as_str) == Some("image_url") {
                had_image = true;
                *part = json!({
                    "type": "text",
                    "text": "[图片已隐藏：当前模型不支持图片输入]"
                });
            }
        }
        if had_image {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            *content = Value::String(if text.is_empty() {
                "[图片已隐藏：当前模型不支持图片输入]".into()
            } else {
                text
            });
        }
    }
}

fn validate_tool_call_batch(
    calls: &[ToolCallPayload],
    advertised: &BTreeSet<&str>,
) -> Result<bool, AgentError> {
    if let Some(call) = calls
        .iter()
        .find(|call| !advertised.contains(call.function.name.as_str()))
    {
        return Err(AgentError::new(
            "tool_not_available",
            format!("当前回合未开放工具：{}", call.function.name),
        ));
    }

    Ok(calls
        .iter()
        .any(|call| call.function.name == READ_SKILL_TOOL_NAME))
}

fn skill_load_calls(calls: &[ToolCallPayload]) -> Vec<&ToolCallPayload> {
    calls
        .iter()
        .filter(|call| call.function.name == READ_SKILL_TOOL_NAME)
        .collect()
}

fn load_skills(
    active_skills: &mut BTreeSet<String>,
    calls: &[&ToolCallPayload],
) -> Result<Vec<(String, String)>, AgentError> {
    let requested = calls
        .iter()
        .map(|call| skills::parse_read_arguments(&call.function.arguments))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(calls
        .iter()
        .zip(requested)
        .map(|(call, skill)| {
            let content = if active_skills.insert(skill.name.clone()) {
                skill.instructions.clone()
            } else {
                format!("SKILL {} 已在当前回合激活，无需重复读取。", skill.name)
            };
            (call.id.clone(), content)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::ToolFunctionPayload;

    fn call(id: &str, name: &str, arguments: &str) -> ToolCallPayload {
        ToolCallPayload {
            id: id.into(),
            r#type: Some("function".into()),
            function: ToolFunctionPayload {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }

    #[test]
    fn tool_batches_allow_skill_loads_with_disclosed_domain_calls() {
        let advertised = BTreeSet::from(["read_skill", "get_editor_snapshot"]);
        assert!(matches!(
            validate_tool_call_batch(
                &[call("1", "preview_add_parameter", "{}")],
                &advertised
            ),
            Err(error) if error.code == "tool_not_available"
        ));
        assert_eq!(
            validate_tool_call_batch(
                &[
                    call("1", "read_skill", r#"{"name":"parameter-editing"}"#),
                    call("2", "get_editor_snapshot", "{}"),
                ],
                &advertised,
            )
            .unwrap(),
            true
        );
        assert_eq!(
            validate_tool_call_batch(
                &[call("1", "read_skill", r#"{"name":"parameter-editing"}"#)],
                &advertised,
            )
            .unwrap(),
            true
        );

        let computer = BTreeSet::from(["perform_computer_action"]);
        assert_eq!(
            validate_tool_call_batch(&[call("1", "perform_computer_action", "{}")], &computer,)
                .unwrap(),
            false
        );
        let mixed_computer = BTreeSet::from(["read_skill", "perform_computer_action"]);
        assert_eq!(
            validate_tool_call_batch(
                &[
                    call("1", "read_skill", r#"{"name":"computer-operation"}"#),
                    call("2", "perform_computer_action", "{}"),
                ],
                &mixed_computer,
            )
            .unwrap(),
            true
        );
        assert_eq!(
            validate_tool_call_batch(
                &[
                    call("1", "perform_computer_action", "{}"),
                    call("2", "perform_computer_action", "{}"),
                ],
                &computer,
            )
            .unwrap(),
            false
        );
    }

    #[test]
    fn skill_loads_are_atomic_and_idempotent() {
        let valid = call("1", "read_skill", r#"{"name":"parameter-editing"}"#);
        let mut active = BTreeSet::new();
        let first = load_skills(&mut active, &[&valid]).unwrap();
        assert!(first[0].1.contains("# Parameter Editing"));
        assert_eq!(active, BTreeSet::from(["parameter-editing".into()]));

        let repeated = load_skills(&mut active, &[&valid]).unwrap();
        assert!(repeated[0].1.contains("无需重复读取"));
        assert_eq!(active.len(), 1);

        let invalid = call("2", "read_skill", r#"{"name":"missing"}"#);
        let mut empty = BTreeSet::new();
        assert!(load_skills(&mut empty, &[&valid, &invalid]).is_err());
        assert!(empty.is_empty());
    }

    #[test]
    fn mixed_batches_only_parse_skill_calls_as_skill_loads() {
        let calls = [
            call("1", "read_skill", r#"{"name":"parameter-editing"}"#),
            call("2", "get_editor_snapshot", "{}"),
        ];
        let skill_calls = skill_load_calls(&calls);
        let mut active = BTreeSet::new();
        let results = load_skills(&mut active, &skill_calls).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "1");
        assert_eq!(active, BTreeSet::from(["parameter-editing".into()]));
    }

    #[test]
    fn tool_failures_are_returned_as_structured_model_context() {
        let content = tool_error_content(&AgentError::new("stale_preview", "preview expired"));
        let value: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "stale_preview");
        assert!(value["error"]["message"].as_str().is_some());
    }

    #[test]
    fn strip_image_content_replaces_image_parts_with_text_placeholders() {
        let mut messages = vec![
            json!({
                "role": "user",
                "content": [
                    { "type": "text", "text": "看这张图" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } }
                ]
            }),
            json!({ "role": "assistant", "content": "好的" }),
            json!({
                "role": "user",
                "content": [
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,BBBB" } }
                ]
            }),
            json!({ "role": "system", "content": "保持不变" }),
        ];
        strip_image_content(&mut messages);

        let first = &messages[0];
        assert_eq!(
            first["content"],
            Value::String("看这张图[图片已隐藏：当前模型不支持图片输入]".into())
        );

        let pure_image = &messages[2];
        assert_eq!(
            pure_image["content"],
            Value::String("[图片已隐藏：当前模型不支持图片输入]".into())
        );

        assert_eq!(messages[1]["content"], "好的");
        assert_eq!(messages[3]["content"], "保持不变");
    }

    #[test]
    fn strip_image_content_leaves_text_only_messages_untouched() {
        let mut messages = vec![json!({ "role": "user", "content": "纯文本消息" })];
        strip_image_content(&mut messages);
        assert_eq!(messages[0]["content"], "纯文本消息");
    }

    #[test]
    fn user_image_messages_become_multimodal_content_and_missing_files_stay_truthful() {
        let dir = std::env::temp_dir().join(format!("nbc-model-image-{}", crate::agent::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.webp");
        std::fs::write(&path, b"webp bytes").unwrap();
        let attachment = crate::agent::images::ChatImageAttachment {
            id: "image".into(),
            name: "image.webp".into(),
            path: path.to_string_lossy().into_owned(),
            mime: "image/webp".into(),
            size: 10,
            available: true,
        };
        let message = crate::agent::store::ChatMessage {
            id: "message".into(),
            role: "user".into(),
            content: "分析图片".into(),
            tool_name: None,
            tool_status: None,
            attachments: vec![attachment.clone()],
            created_at: "now".into(),
        };
        let content = message_content(&message);
        assert_eq!(content[0], json!({ "type": "text", "text": "分析图片" }));
        assert!(content[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/webp;base64,"));

        let missing = crate::agent::store::ChatMessage {
            attachments: vec![crate::agent::images::ChatImageAttachment {
                available: false,
                ..attachment
            }],
            ..message
        };
        assert_eq!(
            message_content(&missing)[1]["text"],
            "[图片不可用：image.webp]"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
