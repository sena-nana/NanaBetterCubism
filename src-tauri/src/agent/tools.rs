use crate::agent::capture::capture_cubism_editor_window;
use crate::agent::skills;
use crate::agent::store::{truncate_summary, MemoryUpsertInput, PendingAsk, PlanStep};
use crate::agent::{emit_conversations_changed, new_id, AgentError, AgentRuntime};
use crate::domain::ParameterBatchInput;
use crate::service::{CommandError, EditorService};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::{atomic::AtomicBool, Arc};
use tauri::{AppHandle, Emitter};

fn all_domain_tool_definitions() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "get_editor_snapshot",
            "获取 Cubism Editor 连接状态、能力门控与参数组摘要。",
            json!({"type": "object", "properties": {}}),
        ),
        tool(
            "connect_editor",
            "连接本机 Cubism Editor External API。",
            json!({
                "type": "object",
                "properties": { "port": { "type": "integer", "minimum": 1, "maximum": 65535 } },
                "required": ["port"]
            }),
        ),
        tool(
            "disconnect_editor",
            "断开 Cubism Editor 连接。",
            json!({"type": "object", "properties": {}}),
        ),
        tool(
            "find_selected_part_parameters",
            "查询 Editor 当前选中 Part 子树关联的参数。",
            json!({"type": "object", "properties": {}}),
        ),
        tool(
            "preview_parameter_batch",
            "校验并预览批量创建参数，返回 previewId。写操作必须先 preview。",
            json!({
                "type": "object",
                "properties": { "input": { "type": "object" } },
                "required": ["input"]
            }),
        ),
        tool(
            "execute_parameter_batch",
            "执行已通过预览的参数批量创建。",
            json!({
                "type": "object",
                "properties": { "previewId": { "type": "string" } },
                "required": ["previewId"]
            }),
        ),
        tool(
            "cancel_parameter_batch",
            "取消进行中的参数批量创建事务。",
            json!({
                "type": "object",
                "properties": { "operationId": { "type": "string" } },
                "required": ["operationId"]
            }),
        ),
        tool(
            "capture_cubism_editor_window",
            "按窗口标题匹配截取 Cubism Editor 窗口。",
            json!({
                "type": "object",
                "properties": {
                    "titleSubstring": { "type": "string", "description": "默认 Cubism Editor" }
                }
            }),
        ),
        tool(
            "list_projects",
            "列出用户手绑的项目。",
            json!({"type": "object", "properties": {}}),
        ),
        tool(
            "bind_conversation_project",
            "将当前对话绑定到项目，或解除绑定。",
            json!({
                "type": "object",
                "properties": {
                    "projectId": { "type": ["string", "null"] },
                    "projectName": { "type": "string", "description": "若提供且无 projectId，则新建项目并绑定" }
                }
            }),
        ),
        tool(
            "list_memories",
            "列出项目阶段记忆与全局经验。",
            json!({
                "type": "object",
                "properties": { "projectId": { "type": ["string", "null"] } }
            }),
        ),
        tool(
            "upsert_memory",
            "写入或更新一条记忆。scope=project|global，kind=stage|experience。",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "scope": { "type": "string" },
                    "kind": { "type": "string" },
                    "projectId": { "type": ["string", "null"] },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "enabled": { "type": "boolean" }
                },
                "required": ["scope", "kind", "title", "body"]
            }),
        ),
        tool(
            "archive_memory",
            "停用一条记忆（enabled=false）。",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        ),
        tool(
            "ask_user",
            "向用户提问并暂停，等待回答后继续。",
            json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "options": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["question"]
            }),
        ),
        tool(
            "update_plan",
            "更新当前对话的计划步骤。",
            json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "status": { "type": "string" }
                            },
                            "required": ["id", "title", "status"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        ),
    ];
    tools.extend(crate::service::official_api::tool_definitions());
    tools
}

pub fn tool_definitions(active_skills: &BTreeSet<String>) -> Result<Vec<Value>, AgentError> {
    let allowed = skills::allowed_domain_tools(active_skills)?;
    let domain_tools = all_domain_tool_definitions();
    let available = domain_tools
        .iter()
        .filter_map(tool_name)
        .collect::<BTreeSet<_>>();
    let missing = allowed.difference(&available).copied().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(AgentError::new(
            "invalid_skill_registry",
            format!("SKILL 引用了未知工具：{}", missing.join(", ")),
        ));
    }

    let mut tools = vec![skills::read_skill_tool_definition()?];
    tools.extend(
        domain_tools
            .into_iter()
            .filter(|definition| tool_name(definition).is_some_and(|name| allowed.contains(name))),
    );
    Ok(tools)
}

#[cfg(test)]
pub fn all_tool_definitions() -> Result<Vec<Value>, AgentError> {
    let domain_tools = all_domain_tool_definitions();
    let available = domain_tools
        .iter()
        .filter_map(tool_name)
        .collect::<BTreeSet<_>>();
    let declared = skills::all_declared_domain_tools()?;
    if available != declared {
        let missing = declared.difference(&available).copied().collect::<Vec<_>>();
        let unassigned = available.difference(&declared).copied().collect::<Vec<_>>();
        return Err(AgentError::new(
            "invalid_skill_registry",
            format!(
                "SKILL 工具注册不完整；未知：{}；未分配：{}",
                missing.join(", "),
                unassigned.join(", ")
            ),
        ));
    }
    let mut tools = vec![skills::read_skill_tool_definition()?];
    tools.extend(domain_tools);
    Ok(tools)
}

pub fn advertised_tool_names(tools: &[Value]) -> BTreeSet<&str> {
    tools.iter().filter_map(tool_name).collect()
}

fn tool_name(definition: &Value) -> Option<&str> {
    definition
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
}

fn tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

pub enum ToolOutcome {
    Result {
        content: String,
        image_path: Option<String>,
    },
    AskUser {
        ask: PendingAsk,
        tool_call_id: String,
    },
}

pub struct ToolExecutionContext<'a> {
    pub app: &'a AppHandle,
    pub runtime: &'a AgentRuntime,
    pub editor: &'a EditorService,
    pub conversation_id: &'a str,
    pub tool_call_id: &'a str,
    pub cancel: Arc<AtomicBool>,
}

fn tool_result(content: impl Into<String>) -> ToolOutcome {
    ToolOutcome::Result {
        content: content.into(),
        image_path: None,
    }
}

pub async fn execute_tool(
    context: ToolExecutionContext<'_>,
    name: &str,
    arguments: &str,
) -> Result<ToolOutcome, AgentError> {
    let ToolExecutionContext {
        app,
        runtime,
        editor,
        conversation_id,
        tool_call_id,
        cancel,
    } = context;
    let args: Value = serde_json::from_str(arguments)
        .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;

    let outcome = match name {
        "get_editor_snapshot" => {
            let snapshot = editor.snapshot().await;
            Ok(tool_result(serde_json::to_string_pretty(&snapshot)?))
        }
        "connect_editor" => {
            let port = args
                .get("port")
                .and_then(Value::as_u64)
                .filter(|port| (1..=65535).contains(port))
                .ok_or_else(|| {
                    AgentError::new("invalid_arguments", "port 必须是 1 到 65535 的整数")
                })? as u16;
            let snapshot = editor
                .start_connection(app.clone(), port)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&snapshot)?))
        }
        "disconnect_editor" => {
            editor.disconnect(app).await.map_err(map_command_error)?;
            Ok(tool_result("已断开连接。"))
        }
        "find_selected_part_parameters" => {
            let result = editor
                .find_part_parameters()
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&result)?))
        }
        "preview_parameter_batch" => {
            let input: ParameterBatchInput =
                serde_json::from_value(args.get("input").cloned().unwrap_or(json!({})))
                    .map_err(|e| AgentError::new("invalid_arguments", e.to_string()))?;
            let preview = editor
                .preview_batch(input)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&preview)?))
        }
        "execute_parameter_batch" => {
            let preview_id = args
                .get("previewId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 previewId"))?
                .to_string();
            let accepted = editor
                .execute_batch(app.clone(), preview_id)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&accepted)?))
        }
        "cancel_parameter_batch" => {
            let operation_id = args
                .get("operationId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 operationId"))?;
            editor
                .cancel_batch(app, operation_id)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result("已请求取消。"))
        }
        "execute_editor_edit" => {
            let preview_id = args
                .get("previewId")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 previewId"))?
                .to_string();
            let accepted = editor
                .execute_editor_edit(app.clone(), preview_id, cancel)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&accepted)?))
        }
        "get_editor_edit_result" => {
            let operation_id = args
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 operationId"))?;
            let result = editor
                .editor_edit_result(operation_id)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&result)?))
        }
        "cancel_editor_edit" => {
            let operation_id = args
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 operationId"))?;
            editor
                .cancel_batch(app, operation_id)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result("已请求取消 Editor 编辑事务。"))
        }
        official if crate::service::official_api::is_tool(official) => {
            let result = crate::service::official_api::call_tool(editor, official, args)
                .await
                .map_err(map_command_error)?;
            Ok(tool_result(serde_json::to_string_pretty(&result)?))
        }
        "capture_cubism_editor_window" => {
            let needle = args
                .get("titleSubstring")
                .and_then(|v| v.as_str())
                .unwrap_or("Cubism Editor");
            let cache = runtime
                .store
                .cache_dir()
                .ok_or_else(|| AgentError::new("store_not_ready", "缓存目录不可用。"))?;
            let captured = capture_cubism_editor_window(&cache, needle)?;
            Ok(ToolOutcome::Result {
                content: serde_json::to_string_pretty(&captured)?,
                image_path: Some(captured.path),
            })
        }
        "list_projects" => {
            let projects = runtime.store.list_projects()?;
            Ok(tool_result(serde_json::to_string_pretty(&projects)?))
        }
        "bind_conversation_project" => {
            let mut project_id = args
                .get("projectId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if project_id.is_none() {
                if let Some(name) = args.get("projectName").and_then(|v| v.as_str()) {
                    let project = runtime.store.upsert_project(None, name.into())?;
                    project_id = Some(project.id);
                }
            }
            runtime
                .store
                .bind_project(conversation_id, project_id.clone())?;
            emit_conversations_changed(app);
            Ok(tool_result(serde_json::to_string_pretty(&json!({
                "projectId": project_id
            }))?))
        }
        "list_memories" => {
            let project_id = args
                .get("projectId")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let memories = runtime.store.list_memories(project_id)?;
            Ok(tool_result(serde_json::to_string_pretty(&memories)?))
        }
        "upsert_memory" => {
            let memory = runtime.store.upsert_memory(MemoryUpsertInput {
                id: args.get("id").and_then(|v| v.as_str()).map(str::to_string),
                scope: args
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .unwrap_or("project")
                    .into(),
                kind: args
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stage")
                    .into(),
                project_id: args
                    .get("projectId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or(runtime.store.conversation_project_id(conversation_id)?),
                title: args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("记忆")
                    .into(),
                body: args
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                enabled: args.get("enabled").and_then(|v| v.as_bool()),
                source_conversation_id: Some(conversation_id.into()),
            })?;
            Ok(tool_result(serde_json::to_string_pretty(&memory)?))
        }
        "archive_memory" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 id"))?;
            runtime.store.set_memory_enabled(id, false)?;
            Ok(tool_result("已停用记忆。"))
        }
        "ask_user" => {
            let question = args
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if question.trim().is_empty() {
                return Err(AgentError::new("invalid_arguments", "question 不能为空"));
            }
            let options = args
                .get("options")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let ask = PendingAsk {
                ask_id: new_id(),
                conversation_id: conversation_id.into(),
                question,
                options,
            };
            runtime.store.set_pending_ask(&ask, tool_call_id)?;
            Ok(ToolOutcome::AskUser {
                ask,
                tool_call_id: tool_call_id.into(),
            })
        }
        "update_plan" => {
            let steps = args
                .get("steps")
                .cloned()
                .map(serde_json::from_value::<Vec<PlanStep>>)
                .transpose()
                .map_err(|e| AgentError::new("invalid_arguments", e.to_string()))?
                .unwrap_or_default();
            let plan = runtime.store.upsert_plan(conversation_id, steps)?;
            let _ = app.emit(
                "agent://plan",
                json!({ "conversationId": conversation_id, "plan": plan }),
            );
            Ok(tool_result(serde_json::to_string_pretty(&plan)?))
        }
        other => Err(AgentError::new(
            "unknown_tool",
            format!("未知工具：{other}"),
        )),
    };

    match &outcome {
        Ok(ToolOutcome::Result { content, .. }) => {
            let summary = truncate_summary(content, 180);
            emit_tool(app, conversation_id, name, "finished", &summary);
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                &summary,
                Some(name),
                Some("finished"),
            );
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                arguments,
                content,
                "finished",
            );
        }
        Ok(ToolOutcome::AskUser { ask, .. }) => {
            emit_tool(app, conversation_id, name, "finished", "等待用户回答");
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                &ask.question,
                Some(name),
                Some("finished"),
            );
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                arguments,
                &ask.question,
                "waiting_user",
            );
        }
        Err(error) => {
            emit_tool(app, conversation_id, name, "failed", &error.message);
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                &error.message,
                Some(name),
                Some("failed"),
            );
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                arguments,
                &error.message,
                "failed",
            );
        }
    }

    outcome
}

fn emit_tool(app: &AppHandle, conversation_id: &str, tool_name: &str, status: &str, summary: &str) {
    let _ = app.emit(
        "agent://tool",
        json!({
            "conversationId": conversation_id,
            "toolName": tool_name,
            "status": status,
            "summary": summary,
        }),
    );
}

fn map_command_error(error: CommandError) -> AgentError {
    AgentError::new(error.code, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(definitions: &[Value]) -> BTreeSet<&str> {
        advertised_tool_names(definitions)
    }

    #[test]
    fn initial_tool_set_only_contains_core_tools() {
        let definitions = tool_definitions(&BTreeSet::new()).unwrap();
        assert_eq!(
            names(&definitions),
            BTreeSet::from([
                "read_skill",
                "get_editor_snapshot",
                "connect_editor",
                "disconnect_editor",
                "ask_user",
                "update_plan",
            ])
        );
    }

    #[test]
    fn active_skills_only_add_their_declared_tools() {
        let parameter = tool_definitions(&BTreeSet::from(["parameter-editing".into()])).unwrap();
        let parameter_names = names(&parameter);
        assert!(parameter_names.contains("preview_add_parameter"));
        assert!(parameter_names.contains("execute_editor_edit"));
        assert!(!parameter_names.contains("preview_add_part"));
        assert!(!parameter_names.contains("list_memories"));

        let combined = tool_definitions(&BTreeSet::from([
            "parameter-editing".into(),
            "object-editing".into(),
        ]))
        .unwrap();
        let combined_names = names(&combined);
        assert!(combined_names.contains("preview_add_parameter"));
        assert!(combined_names.contains("preview_add_part"));
        assert_eq!(
            combined_names
                .iter()
                .filter(|name| **name == "execute_editor_edit")
                .count(),
            1
        );
    }

    #[test]
    fn every_domain_tool_is_core_or_assigned_to_a_skill() {
        let definitions = all_tool_definitions().unwrap();
        assert_eq!(names(&definitions).len(), definitions.len());
        assert!(names(&definitions).contains("read_skill"));
        for forbidden in ["read_file", "write_file", "apply_patch", "run_terminal"] {
            assert!(!names(&definitions).contains(forbidden));
        }
    }
}
