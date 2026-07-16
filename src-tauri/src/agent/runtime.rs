use crate::agent::computer_control::ComputerOperationStatus;
use crate::agent::llm::{
    chat_completions_stream, content_to_text, image_file_to_data_url, ToolCallPayload,
};
use crate::agent::skills::{self, MAX_SKILL_LOAD_STEPS, READ_SKILL_TOOL_NAME};
use crate::agent::tools::{
    advertised_tool_names, execute_tool, tool_definitions, ToolExecutionContext, ToolOutcome,
};
use crate::agent::{
    emit_conversations_changed, AgentError, AgentRuntime, AgentTurnState, PendingContinuation,
    SYSTEM_PROMPT,
};
use crate::service::EditorService;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

const MAX_ACTION_STEPS: usize = 12;
const MAX_COMPUTER_ACTION_STEPS: usize = 36;

pub async fn run_turn(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    conversation_id: String,
    user_text: String,
    cancel: Arc<AtomicBool>,
) -> Result<(), AgentError> {
    let editor = app.state::<EditorService>();
    let result = run_turn_inner(
        &app,
        &runtime,
        editor.inner(),
        &conversation_id,
        Some(user_text),
        None,
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
            None,
            Some(state),
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
            None,
            Some(state),
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

async fn run_turn_inner(
    app: &AppHandle,
    runtime: &AgentRuntime,
    editor: &EditorService,
    conversation_id: &str,
    user_text: Option<String>,
    existing_state: Option<AgentTurnState>,
    cancel: Arc<AtomicBool>,
) -> Result<TurnEnd, AgentError> {
    let config = runtime.store.get_llm_config()?;
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
        for item in runtime.store.get_messages(conversation_id)? {
            if item.role == "tool" {
                continue;
            }
            seeded.push(json!({
                "role": item.role,
                "content": item.content,
            }));
        }
        if let Some(text) = user_text {
            runtime
                .store
                .append_message(conversation_id, "user", &text, None, None)?;
            let title: String = text.chars().take(24).collect();
            let _ = runtime
                .store
                .set_conversation_title_if_default(conversation_id, &title);
            emit_conversations_changed(app);
            seeded.push(json!({
                "role": "user",
                "content": text,
            }));
        }
        AgentTurnState::new(seeded)
    };

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(AgentError::new("cancelled", "已取消。"));
        }

        let tools = tool_definitions(&state.active_skills)?;
        let advertised = advertised_tool_names(&tools);
        let assistant = {
            let conversation_id = conversation_id.to_string();
            let app = app.clone();
            chat_completions_stream(&config, &state.messages, &tools, move |piece| {
                let _ = app.emit(
                    "agent://turn-delta",
                    json!({ "conversationId": conversation_id, "text": piece }),
                );
            })
            .await?
        };
        let text = content_to_text(&assistant.content);
        if !text.is_empty() {
            let _ = runtime
                .store
                .append_message(conversation_id, "assistant", &text, None, None);
        }

        let tool_calls = assistant.tool_calls.clone().unwrap_or_default();
        if tool_calls.is_empty() {
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

        let batch = validate_tool_call_batch(&tool_calls, &advertised)?;
        if batch.includes_skill_load && state.skill_load_steps >= MAX_SKILL_LOAD_STEPS {
            return Err(AgentError::new(
                "step_limit",
                "达到 SKILL 读取步数上限，已停止。",
            ));
        }
        if batch.action_kind == Some(ActionKind::Domain) && state.action_steps >= MAX_ACTION_STEPS {
            return Err(AgentError::new(
                "step_limit",
                "达到工具调用步数上限，已停止。",
            ));
        }
        if batch.action_kind == Some(ActionKind::Computer)
            && state.computer_action_steps >= MAX_COMPUTER_ACTION_STEPS
        {
            return Err(AgentError::new(
                "step_limit",
                "达到电脑代理操作步数上限，已停止。",
            ));
        }

        if batch.includes_skill_load {
            state.skill_load_steps += 1;
        }
        match batch.action_kind {
            Some(ActionKind::Domain) => state.action_steps += 1,
            Some(ActionKind::Computer) => state.computer_action_steps += 1,
            None => {}
        }

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

        if batch.includes_skill_load {
            let skill_calls = skill_load_calls(&tool_calls);
            for (tool_call_id, content) in load_skills(&mut state.active_skills, &skill_calls)? {
                state.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                }));
            }
        }
        if batch.action_kind.is_none() {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    Domain,
    Computer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ToolCallBatch {
    includes_skill_load: bool,
    action_kind: Option<ActionKind>,
}

fn validate_tool_call_batch(
    calls: &[ToolCallPayload],
    advertised: &BTreeSet<&str>,
) -> Result<ToolCallBatch, AgentError> {
    if let Some(call) = calls
        .iter()
        .find(|call| !advertised.contains(call.function.name.as_str()))
    {
        return Err(AgentError::new(
            "tool_not_available",
            format!("当前回合未开放工具：{}", call.function.name),
        ));
    }

    let skill_calls = calls
        .iter()
        .filter(|call| call.function.name == READ_SKILL_TOOL_NAME)
        .count();
    let computer_calls = calls
        .iter()
        .filter(|call| crate::agent::tools::is_computer_tool(&call.function.name))
        .count();
    if computer_calls > 0 && (skill_calls > 0 || computer_calls != 1 || calls.len() != 1) {
        Err(AgentError::new(
            "mixed_computer_action",
            "电脑代理工具每次只能调用一个，且不能与其他工具混用。",
        ))
    } else {
        Ok(ToolCallBatch {
            includes_skill_load: skill_calls > 0,
            action_kind: if computer_calls == 1 {
                Some(ActionKind::Computer)
            } else if skill_calls < calls.len() {
                Some(ActionKind::Domain)
            } else {
                None
            },
        })
    }
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
            ToolCallBatch {
                includes_skill_load: true,
                action_kind: Some(ActionKind::Domain),
            }
        );
        assert_eq!(
            validate_tool_call_batch(
                &[call("1", "read_skill", r#"{"name":"parameter-editing"}"#)],
                &advertised,
            )
            .unwrap(),
            ToolCallBatch {
                includes_skill_load: true,
                action_kind: None,
            }
        );

        let computer = BTreeSet::from(["perform_computer_action"]);
        assert_eq!(
            validate_tool_call_batch(&[call("1", "perform_computer_action", "{}")], &computer,)
                .unwrap(),
            ToolCallBatch {
                includes_skill_load: false,
                action_kind: Some(ActionKind::Computer),
            }
        );
        let mixed_computer = BTreeSet::from(["read_skill", "perform_computer_action"]);
        assert!(matches!(
            validate_tool_call_batch(
                &[
                    call("1", "read_skill", r#"{"name":"computer-operation"}"#),
                    call("2", "perform_computer_action", "{}"),
                ],
                &mixed_computer,
            ),
            Err(error) if error.code == "mixed_computer_action"
        ));
        assert!(matches!(
            validate_tool_call_batch(
                &[
                    call("1", "perform_computer_action", "{}"),
                    call("2", "perform_computer_action", "{}"),
                ],
                &computer,
            ),
            Err(error) if error.code == "mixed_computer_action"
        ));
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
}
