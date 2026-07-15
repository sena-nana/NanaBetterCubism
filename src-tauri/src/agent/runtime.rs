use crate::agent::llm::{
    chat_completions, chat_completions_stream, content_to_text, image_file_to_data_url,
    ToolCallPayload,
};
use crate::agent::skills::{self, MAX_SKILL_LOAD_STEPS, READ_SKILL_TOOL_NAME};
use crate::agent::store::MemoryUpsertInput;
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

    let result = finalize_turn(&runtime, &conversation_id, &cancel, result).await;
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

pub async fn continue_after_ask(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    ask_id: String,
    conversation_id: String,
    answer: String,
    cancel: Arc<AtomicBool>,
) -> Result<(), AgentError> {
    let result = async {
        let (ask, _tool_call_id) = runtime
            .store
            .take_pending_ask(&ask_id)?
            .ok_or_else(|| AgentError::new("ask_not_found", "没有等待中的提问。"))?;
        if ask.conversation_id != conversation_id {
            return Err(AgentError::new("ask_not_found", "提问上下文已失效。"));
        }

        let continuation = runtime
            .pending_continuations
            .lock()
            .await
            .remove(&ask.ask_id)
            .ok_or_else(|| AgentError::new("ask_not_found", "提问上下文已失效。"))?;

        let state = continuation.resume(&answer);
        runtime.store.append_message(
            &ask.conversation_id,
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

    let result = finalize_turn(&runtime, &conversation_id, &cancel, result).await;
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

enum TurnEnd {
    Finished,
    WaitingAsk,
}

async fn finalize_turn(
    runtime: &AgentRuntime,
    conversation_id: &str,
    cancel: &Arc<AtomicBool>,
    result: Result<TurnEnd, AgentError>,
) -> Result<TurnEnd, AgentError> {
    if !runtime.finish_turn(conversation_id, cancel).await || result.is_err() {
        return result;
    }
    if matches!(&result, Ok(TurnEnd::WaitingAsk)) {
        runtime.clear_pending_ask(conversation_id).await?;
    }
    Err(AgentError::new("cancelled", "已取消。"))
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
        Ok(TurnEnd::WaitingAsk) => {
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
        let project_id = runtime.store.conversation_project_id(conversation_id)?;
        let memories = runtime
            .store
            .memories_for_injection(project_id.as_deref())?;
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
        if !memories.is_empty() {
            let memory_text = memories
                .iter()
                .map(|item| {
                    format!(
                        "- [{} / {}] {}: {}",
                        item.scope, item.kind, item.title, item.body
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            seeded.push(json!({
                "role": "system",
                "content": format!("相关记忆：\n{memory_text}")
            }));
        }
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
            return Ok(TurnEnd::Finished);
        }

        let batch = validate_tool_call_batch(&tool_calls, &advertised)?;
        match batch {
            ToolCallBatch::SkillLoad => {
                if state.skill_load_steps >= MAX_SKILL_LOAD_STEPS {
                    return Err(AgentError::new(
                        "step_limit",
                        "达到 SKILL 读取步数上限，已停止。",
                    ));
                }
                state.skill_load_steps += 1;
            }
            ToolCallBatch::Action => {
                if state.action_steps >= MAX_ACTION_STEPS {
                    return Err(AgentError::new(
                        "step_limit",
                        "达到工具调用步数上限，已停止。",
                    ));
                }
                state.action_steps += 1;
            }
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

        if batch == ToolCallBatch::SkillLoad {
            for (tool_call_id, content) in load_skills(&mut state.active_skills, &tool_calls)? {
                state.messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                }));
            }
            continue;
        }

        for call in tool_calls {
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
                ToolOutcome::AskUser { ask, tool_call_id } => {
                    runtime.pending_continuations.lock().await.insert(
                        ask.ask_id.clone(),
                        PendingContinuation {
                            conversation_id: conversation_id.into(),
                            tool_call_id,
                            state,
                        },
                    );
                    let _ = app.emit(
                        "agent://ask",
                        json!({ "conversationId": conversation_id, "ask": ask }),
                    );
                    return Ok(TurnEnd::WaitingAsk);
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
enum ToolCallBatch {
    SkillLoad,
    Action,
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
    if skill_calls == 0 {
        Ok(ToolCallBatch::Action)
    } else if skill_calls == calls.len() {
        Ok(ToolCallBatch::SkillLoad)
    } else {
        Err(AgentError::new(
            "mixed_skill_load",
            "read_skill 不能与业务工具在同一响应中调用。",
        ))
    }
}

fn load_skills(
    active_skills: &mut BTreeSet<String>,
    calls: &[ToolCallPayload],
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

pub async fn consolidate_memory(
    app: AppHandle,
    runtime: &AgentRuntime,
    conversation_id: &str,
) -> Result<(), AgentError> {
    let config = runtime.store.get_llm_config()?;
    let project_id = runtime.store.conversation_project_id(conversation_id)?;
    let history = runtime
        .store
        .get_messages(conversation_id)?
        .into_iter()
        .map(|item| format!("{}: {}", item.role, item.content))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "请根据对话整理记忆，返回 JSON：{{\"stageTitle\":\"\",\"stageBody\":\"\",\"experienceTitle\":\"\",\"experienceBody\":\"\"}}。\n\
         stage 是本项目阶段事实；experience 是可迁移 Live2D 经验。若某项无内容，对应字符串留空。\n\n对话：\n{history}"
    );
    let messages = vec![
        json!({"role": "system", "content": "你只输出合法 JSON 对象。"}),
        json!({"role": "user", "content": prompt}),
    ];
    let response = chat_completions(&config, &messages, &[]).await?;
    let text = content_to_text(&response.content);
    let cleaned = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: Value = serde_json::from_str(cleaned).unwrap_or(json!({}));

    if let Some(project_id) = project_id {
        let stage_title = parsed
            .get("stageTitle")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let stage_body = parsed
            .get("stageBody")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if !stage_body.is_empty() {
            runtime.store.upsert_memory(MemoryUpsertInput {
                id: None,
                scope: "project".into(),
                kind: "stage".into(),
                project_id: Some(project_id),
                title: if stage_title.is_empty() {
                    "阶段记录".into()
                } else {
                    stage_title.into()
                },
                body: stage_body.into(),
                enabled: Some(true),
                source_conversation_id: Some(conversation_id.into()),
            })?;
        }
    }

    let experience_title = parsed
        .get("experienceTitle")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let experience_body = parsed
        .get("experienceBody")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !experience_body.is_empty() {
        runtime.store.upsert_memory(MemoryUpsertInput {
            id: None,
            scope: "global".into(),
            kind: "experience".into(),
            project_id: None,
            title: if experience_title.is_empty() {
                "Live2D 经验".into()
            } else {
                experience_title.into()
            },
            body: experience_body.into(),
            enabled: Some(true),
            source_conversation_id: Some(conversation_id.into()),
        })?;
    }

    emit_conversations_changed(&app);
    Ok(())
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
    fn tool_batches_reject_hidden_and_mixed_calls_before_execution() {
        let advertised = BTreeSet::from(["read_skill", "get_editor_snapshot"]);
        assert!(matches!(
            validate_tool_call_batch(
                &[call("1", "preview_add_parameter", "{}")],
                &advertised
            ),
            Err(error) if error.code == "tool_not_available"
        ));
        assert!(matches!(
            validate_tool_call_batch(
                &[
                    call("1", "read_skill", r#"{"name":"parameter-editing"}"#),
                    call("2", "get_editor_snapshot", "{}"),
                ],
                &advertised,
            ),
            Err(error) if error.code == "mixed_skill_load"
        ));
        assert_eq!(
            validate_tool_call_batch(
                &[call("1", "read_skill", r#"{"name":"parameter-editing"}"#)],
                &advertised,
            )
            .unwrap(),
            ToolCallBatch::SkillLoad
        );
    }

    #[test]
    fn skill_loads_are_atomic_and_idempotent() {
        let valid = call("1", "read_skill", r#"{"name":"parameter-editing"}"#);
        let mut active = BTreeSet::new();
        let first = load_skills(&mut active, std::slice::from_ref(&valid)).unwrap();
        assert!(first[0].1.contains("# Parameter Editing"));
        assert_eq!(active, BTreeSet::from(["parameter-editing".into()]));

        let repeated = load_skills(&mut active, std::slice::from_ref(&valid)).unwrap();
        assert!(repeated[0].1.contains("无需重复读取"));
        assert_eq!(active.len(), 1);

        let invalid = call("2", "read_skill", r#"{"name":"missing"}"#);
        let mut empty = BTreeSet::new();
        assert!(load_skills(&mut empty, &[valid, invalid]).is_err());
        assert!(empty.is_empty());
    }

    #[test]
    fn tool_failures_are_returned_as_structured_model_context() {
        let content = tool_error_content(&AgentError::new(
            "stale_preview",
            "preview expired",
        ));
        let value: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "stale_preview");
        assert!(value["error"]["message"].as_str().is_some());
    }
}
