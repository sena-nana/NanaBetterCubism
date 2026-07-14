use crate::agent::llm::{
    chat_completions, chat_completions_stream, content_to_text, image_file_to_data_url,
};
use crate::agent::store::MemoryUpsertInput;
use crate::agent::tools::{execute_tool, tool_definitions, ToolOutcome};
use crate::agent::{AgentError, AgentRuntime, PendingContinuation, SYSTEM_PROMPT};
use crate::service::EditorService;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

const MAX_STEPS: usize = 12;

pub async fn run_turn(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    conversation_id: String,
    user_text: String,
) -> Result<(), AgentError> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut flags = runtime.cancel_flags.lock().await;
        flags.insert(conversation_id.clone(), cancel.clone());
    }

    let editor = app.state::<EditorService>();
    let result = run_turn_inner(
        &app,
        &runtime,
        editor.inner(),
        &conversation_id,
        Some(user_text),
        None,
        cancel,
    )
    .await;

    runtime.cancel_flags.lock().await.remove(&conversation_id);
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

pub async fn continue_after_ask(
    app: AppHandle,
    runtime: Arc<AgentRuntime>,
    ask_id: String,
    answer: String,
) -> Result<(), AgentError> {
    let (ask, _tool_call_id) = runtime
        .store
        .take_pending_ask(&ask_id)?
        .ok_or_else(|| AgentError::new("ask_not_found", "没有等待中的提问。"))?;

    let continuation = {
        let mut pending = runtime.pending_continuations.lock().await;
        pending.remove(&ask.ask_id)
    }
    .ok_or_else(|| AgentError::new("ask_not_found", "提问上下文已失效。"))?;

    let mut messages = continuation.messages;
    messages.push(json!({
        "role": "tool",
        "tool_call_id": continuation.tool_call_id,
        "content": answer.clone(),
    }));
    let _ = runtime.store.append_message(
        &ask.conversation_id,
        "user",
        &format!("回答：{answer}"),
        None,
        None,
    );

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut flags = runtime.cancel_flags.lock().await;
        flags.insert(ask.conversation_id.clone(), cancel.clone());
    }

    let conversation_id = ask.conversation_id.clone();
    let editor = app.state::<EditorService>();
    let result = run_turn_inner(
        &app,
        &runtime,
        editor.inner(),
        &conversation_id,
        None,
        Some(messages),
        cancel,
    )
    .await;

    runtime.cancel_flags.lock().await.remove(&conversation_id);
    emit_finished(&app, &conversation_id, &result);
    result.map(|_| ())
}

enum TurnEnd {
    Finished,
    WaitingAsk,
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
            let _ = app.emit("agent://conversations-changed", json!({}));
        }
        Ok(TurnEnd::WaitingAsk) => {}
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
    existing_messages: Option<Vec<Value>>,
    cancel: Arc<AtomicBool>,
) -> Result<TurnEnd, AgentError> {
    let config = runtime.store.get_llm_config()?;
    let project_id = runtime.store.conversation_project_id(conversation_id)?;
    let memories = runtime
        .store
        .memories_for_injection(project_id.as_deref())?;

    let mut messages = if let Some(existing) = existing_messages {
        existing
    } else {
        let mut seeded = vec![json!({
            "role": "system",
            "content": SYSTEM_PROMPT
        })];
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
            seeded.push(json!({
                "role": "user",
                "content": text,
            }));
        }
        seeded
    };

    let tools = tool_definitions();

    for _ in 0..MAX_STEPS {
        if cancel.load(Ordering::SeqCst) {
            return Err(AgentError::new("cancelled", "已取消。"));
        }

        let assistant = {
            let conversation_id = conversation_id.to_string();
            let app = app.clone();
            chat_completions_stream(&config, &messages, &tools, move |piece| {
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

        messages.push(json!({
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

        for call in tool_calls {
            if cancel.load(Ordering::SeqCst) {
                return Err(AgentError::new("cancelled", "已取消。"));
            }
            let outcome = execute_tool(
                app,
                runtime,
                editor,
                conversation_id,
                &call.id,
                &call.function.name,
                &call.function.arguments,
            )
            .await?;

            match outcome {
                ToolOutcome::AskUser { ask, tool_call_id } => {
                    runtime.pending_continuations.lock().await.insert(
                        ask.ask_id.clone(),
                        PendingContinuation {
                            conversation_id: conversation_id.into(),
                            tool_call_id,
                            messages: messages.clone(),
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
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": content,
                    }));
                    if let Some(path) = image_path {
                        match image_file_to_data_url(&path) {
                            Ok(data_url) => {
                                messages.push(json!({
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
                                messages.push(json!({
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

    Err(AgentError::new(
        "step_limit",
        "达到工具调用步数上限，已停止。",
    ))
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

    let _ = app.emit("agent://conversations-changed", json!({}));
    Ok(())
}
