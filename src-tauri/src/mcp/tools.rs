use crate::agent::psd::{attachment_manifest, ChatPsdDocument, PsdService, MAX_PSD_DOCUMENTS_PER_CONVERSATION};
use crate::agent::tools::{
    all_tool_definitions, execute_tool, tool_name, ToolAccess as AgentToolAccess,
    ToolExecutionContext, ToolOutcome,
};
use crate::agent::{AgentError, AgentRuntime, AgentTurnMode};
use crate::service::official_api::{self, ToolAccess};
use crate::service::{CommandError, EditorService};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::model::{
    CallToolResult, ContentBlock, Tool, ToolAnnotations,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

pub(crate) const MCP_PSD_SESSION: &str = "mcp";

#[derive(Clone)]
pub(crate) struct McpToolContext {
    pub app: AppHandle,
    pub editor: EditorService,
    pub psd: Arc<PsdService>,
    pub psd_documents: Arc<Mutex<Vec<ChatPsdDocument>>>,
    pub allow_writes: Arc<Mutex<bool>>,
    pub agent_runtime: Option<Arc<AgentRuntime>>,
    pub conversation_id: Option<String>,
    pub turn_mode: Arc<Mutex<AgentTurnMode>>,
    pub turn_cancel: Arc<Mutex<Arc<AtomicBool>>>,
}

#[derive(Clone)]
struct ToolMeta {
    name: String,
    description: String,
    parameters: Value,
    access: ToolAccess,
}

pub(crate) fn list_mcp_tools(allow_writes: bool) -> Vec<Tool> {
    catalog()
        .into_iter()
        .filter(|tool| allow_writes || !is_write_gated(&tool.name, tool.access))
        .map(|tool| {
            let schema = parameters_object(&tool.parameters);
            let annotations = ToolAnnotations::new()
                .read_only(tool.access == ToolAccess::ReadOnly)
                .destructive(tool.access == ToolAccess::Mutating)
                .idempotent(tool.access == ToolAccess::ReadOnly)
                .open_world(false);
            Tool::new(tool.name.clone(), tool.description.clone(), schema)
                .with_annotations(annotations)
        })
        .collect()
}

pub(crate) fn list_internal_tools() -> Result<Vec<Tool>, AgentError> {
    all_tool_definitions()?
        .into_iter()
        .filter(|definition| {
            tool_name(definition).is_some_and(|name| {
                !matches!(
                    name,
                    "ask_user"
                        | "submit_plan"
                        | "update_plan"
                        | "read_skill"
                        | "list_cubism_windows"
                        | "request_computer_operation"
                        | "capture_cubism_editor_window"
                )
            })
        })
        .map(|definition| {
            let function = definition
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(|| AgentError::new("invalid_tool_schema", "工具定义缺少 function。"))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::new("invalid_tool_schema", "工具定义缺少名称。"))?;
            let description = function
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let parameters = function
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            let access = crate::agent::tools::tool_access(name).unwrap_or(AgentToolAccess::ReadOnly);
            let annotations = ToolAnnotations::new()
                .read_only(access == AgentToolAccess::ReadOnly)
                .destructive(access == AgentToolAccess::Mutating)
                .idempotent(access == AgentToolAccess::ReadOnly)
                .open_world(false);
            Ok(
                Tool::new(name.to_string(), description.to_string(), parameters_object(&parameters))
                    .with_annotations(annotations),
            )
        })
        .collect()
}

pub(crate) async fn call_internal_tool(
    context: &McpToolContext,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, AgentError> {
    let runtime = context
        .agent_runtime
        .as_ref()
        .ok_or_else(|| AgentError::new("internal_mcp_context", "内部 MCP 会话未绑定 Agent。"))?;
    let conversation_id = context
        .conversation_id
        .as_deref()
        .ok_or_else(|| AgentError::new("internal_mcp_context", "内部 MCP 会话缺少对话。"))?;
    if !list_internal_tools()?
        .iter()
        .any(|tool| tool.name.as_ref() == name)
    {
        return Err(AgentError::new("unknown_tool", format!("未知工具：{name}")));
    }
    let mode = *context.turn_mode.lock().unwrap();
    let cancel = context.turn_cancel.lock().unwrap().clone();
    let arguments = Value::Object(arguments.unwrap_or_default()).to_string();
    let outcome = execute_tool(
        ToolExecutionContext {
            app: &context.app,
            runtime,
            editor: &context.editor,
            conversation_id,
            tool_call_id: &crate::agent::new_id(),
            cancel,
            mode,
            computer_permission_denied: true,
        },
        name,
        &arguments,
    )
    .await?;
    match outcome {
        ToolOutcome::Result {
            content,
            image_path,
        } => {
            let mut blocks = vec![ContentBlock::text(content)];
            if let Some(path) = image_path {
                let bytes = std::fs::read(path)
                    .map_err(|_| AgentError::new("image_unavailable", "无法读取工具图片结果。"))?;
                blocks.push(ContentBlock::image(BASE64.encode(bytes), "image/png"));
            }
            Ok(CallToolResult::success(blocks))
        }
        ToolOutcome::AwaitUser { .. } => Err(AgentError::new(
            "unexpected_user_action",
            "内部 MCP 工具不能直接创建应用内提问。",
        )),
        ToolOutcome::PlanSubmitted(_) => Err(AgentError::new(
            "unexpected_plan",
            "内部 MCP 工具不能直接提交计划。",
        )),
    }
}

fn is_write_gated(name: &str, access: ToolAccess) -> bool {
    access == ToolAccess::Mutating
        && !matches!(
            name,
            "connect_editor" | "disconnect_editor" | "attach_psd" | "detach_psd"
        )
}

pub(crate) async fn call_mcp_tool(
    context: &McpToolContext,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, AgentError> {
    let allow_writes = *context.allow_writes.lock().unwrap();
    let meta = catalog_map()
        .get(name)
        .cloned()
        .ok_or_else(|| AgentError::new("unknown_tool", format!("未知工具：{name}")))?;
    if !allow_writes && is_write_gated(name, meta.access) {
        return Err(AgentError::new(
            "writes_disabled",
            "MCP 已关闭写入；请在设置中开启「允许写入」。",
        ));
    }
    let args = Value::Object(arguments.unwrap_or_default());
    match name {
        "get_editor_snapshot" => {
            let snapshot = context.editor.snapshot().await;
            ok_json(&snapshot)
        }
        "connect_editor" => {
            let port = args
                .get("port")
                .and_then(Value::as_u64)
                .filter(|port| (1..=65535).contains(port))
                .ok_or_else(|| {
                    AgentError::new("invalid_arguments", "port 必须是 1 到 65535 的整数")
                })? as u16;
            let snapshot = context
                .editor
                .start_connection(context.app.clone(), port)
                .await
                .map_err(map_command_error)?;
            ok_json(&snapshot)
        }
        "disconnect_editor" => {
            context
                .editor
                .disconnect(&context.app)
                .await
                .map_err(map_command_error)?;
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "已断开连接。",
            )]))
        }
        "find_selected_part_parameters" => {
            let result = context
                .editor
                .find_part_parameters()
                .await
                .map_err(map_command_error)?;
            ok_json(&result)
        }
        "execute_editor_edit" => {
            let preview_id = required_string(&args, "previewId")?;
            let cancel = Arc::new(AtomicBool::new(false));
            let accepted = context
                .editor
                .execute_editor_edit(context.app.clone(), preview_id, cancel)
                .await
                .map_err(map_command_error)?;
            ok_json(&accepted)
        }
        "get_editor_edit_result" => {
            let operation_id = required_string(&args, "operationId")?;
            let result = context
                .editor
                .editor_edit_result(&operation_id)
                .await
                .map_err(map_command_error)?;
            ok_json(&result)
        }
        "cancel_editor_edit" => {
            let operation_id = required_string(&args, "operationId")?;
            context
                .editor
                .cancel_batch(&context.app, &operation_id)
                .await
                .map_err(map_command_error)?;
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "已请求取消 Editor 编辑事务。",
            )]))
        }
        "attach_psd" => {
            let path = required_string(&args, "path")?;
            let mut documents = context.psd_documents.lock().unwrap();
            if documents.len() >= MAX_PSD_DOCUMENTS_PER_CONVERSATION {
                return Err(AgentError::new(
                    "psd_limit",
                    format!("MCP 会话最多附加 {MAX_PSD_DOCUMENTS_PER_CONVERSATION} 个 PSD。"),
                ));
            }
            let (document, structure) = context.psd.load(MCP_PSD_SESSION, &path)?;
            documents.push(document.clone());
            ok_json(&json!({
                "document": attachment_manifest(std::slice::from_ref(&document)).documents[0],
                "structure": structure,
            }))
        }
        "detach_psd" => {
            let psd_id = required_string(&args, "psdId")?;
            let mut documents = context.psd_documents.lock().unwrap();
            if !documents.iter().any(|document| document.id == psd_id) {
                return Err(AgentError::new("psd_unavailable", "未找到该 PSD。"));
            }
            context.psd.discard(&psd_id, MCP_PSD_SESSION)?;
            documents.retain(|document| document.id != psd_id);
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "已移除 PSD。",
            )]))
        }
        "list_attached_psds" => {
            let documents = context.psd_documents.lock().unwrap().clone();
            ok_json(&attachment_manifest(&documents))
        }
        "read_psd_structure" => {
            let psd_id = required_string(&args, "psdId")?;
            ensure_psd_attached(context, &psd_id)?;
            let structure = context.psd.read_structure(&psd_id, MCP_PSD_SESSION)?;
            ok_json(&structure)
        }
        "read_psd_layer_image" => {
            let psd_id = required_string(&args, "psdId")?;
            let layer_id = required_string(&args, "layerId")?;
            ensure_psd_attached(context, &psd_id)?;
            let path = context
                .psd
                .extract_layer_image(&psd_id, MCP_PSD_SESSION, &layer_id)?;
            let bytes = std::fs::read(&path)
                .map_err(|_| AgentError::new("psd_io_error", "无法读取图层画面。"))?;
            let data = BASE64.encode(bytes);
            Ok(CallToolResult::success(vec![
                ContentBlock::text(
                    json!({
                        "psdId": psd_id,
                        "layerId": layer_id,
                    })
                    .to_string(),
                ),
                ContentBlock::image(data, "image/png"),
            ]))
        }
        official if official_api::is_tool(official) => {
            let result = official_api::call_tool(&context.editor, official, args)
                .await
                .map_err(map_command_error)?;
            ok_json(&result)
        }
        _ => Err(AgentError::new("unknown_tool", format!("未知工具：{name}"))),
    }
}

fn ensure_psd_attached(context: &McpToolContext, psd_id: &str) -> Result<(), AgentError> {
    let documents = context.psd_documents.lock().unwrap();
    if documents.iter().any(|document| document.id == psd_id) {
        Ok(())
    } else {
        Err(AgentError::new("psd_unavailable", "未找到该 PSD，请先 attach_psd。"))
    }
}

fn catalog() -> Vec<ToolMeta> {
    let mut tools = vec![
        ToolMeta {
            name: "get_editor_snapshot".into(),
            description: "获取 Cubism Editor 连接状态、能力与模型摘要。".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            access: ToolAccess::ReadOnly,
        },
        ToolMeta {
            name: "connect_editor".into(),
            description: "连接本机 Cubism Editor External API WebSocket 端口。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "port": { "type": "integer", "minimum": 1, "maximum": 65535 }
                },
                "required": ["port"],
                "additionalProperties": false
            }),
            access: ToolAccess::Mutating,
        },
        ToolMeta {
            name: "disconnect_editor".into(),
            description: "断开 Cubism Editor 连接。".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            access: ToolAccess::Mutating,
        },
        ToolMeta {
            name: "find_selected_part_parameters".into(),
            description: "查询当前选中 Part 子树关联的参数。".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            access: ToolAccess::ReadOnly,
        },
        ToolMeta {
            name: "attach_psd".into(),
            description: "将本机 PSD 文件附加到 MCP 会话，返回文档摘要与结构。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "minLength": 1 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            access: ToolAccess::Mutating,
        },
        ToolMeta {
            name: "detach_psd".into(),
            description: "从 MCP 会话移除已附加的 PSD。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "psdId": { "type": "string", "minLength": 1 }
                },
                "required": ["psdId"],
                "additionalProperties": false
            }),
            access: ToolAccess::Mutating,
        },
        ToolMeta {
            name: "list_attached_psds".into(),
            description: "列出 MCP 会话已附加的 PSD（不含真实路径）。".into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            access: ToolAccess::ReadOnly,
        },
        ToolMeta {
            name: "read_psd_structure".into(),
            description: "读取已附加 PSD 的图层树结构。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "psdId": { "type": "string", "minLength": 1 }
                },
                "required": ["psdId"],
                "additionalProperties": false
            }),
            access: ToolAccess::ReadOnly,
        },
        ToolMeta {
            name: "read_psd_layer_image".into(),
            description: "提取 PSD 图层像素画面（返回 PNG 图片内容）。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "psdId": { "type": "string", "minLength": 1 },
                    "layerId": { "type": "string", "minLength": 1 }
                },
                "required": ["psdId", "layerId"],
                "additionalProperties": false
            }),
            access: ToolAccess::ReadOnly,
        },
    ];

    for definition in official_api::tool_definitions() {
        let name = definition
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .expect("官方工具必须有名称")
            .to_string();
        let description = definition
            .get("function")
            .and_then(|function| function.get("description"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let parameters = definition
            .get("function")
            .and_then(|function| function.get("parameters"))
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
        let access = official_api::tool_access(&name).expect("官方工具必须声明访问属性");
        tools.push(ToolMeta {
            name,
            description,
            parameters,
            access,
        });
    }
    tools
}

fn catalog_map() -> BTreeMap<String, ToolMeta> {
    catalog()
        .into_iter()
        .map(|tool| (tool.name.clone(), tool))
        .collect()
}

fn parameters_object(parameters: &Value) -> Arc<Map<String, Value>> {
    match parameters {
        Value::Object(map) => Arc::new(map.clone()),
        _ => Arc::new(Map::from_iter([(
            "type".into(),
            Value::String("object".into()),
        )])),
    }
}

fn required_string(args: &Value, key: &str) -> Result<String, AgentError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AgentError::new("invalid_arguments", format!("缺少 {key}")))
}

fn ok_json<T: serde::Serialize>(value: &T) -> Result<CallToolResult, AgentError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| AgentError::new("json_error", error.to_string()))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn map_command_error(error: CommandError) -> AgentError {
    AgentError::new(error.code, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_filter_hides_mutating_tools() {
        let all = list_mcp_tools(true);
        let read_only = list_mcp_tools(false);
        assert!(all.iter().any(|tool| tool.name == "execute_editor_edit"));
        assert!(all.iter().any(|tool| tool.name == "attach_psd"));
        assert!(all.iter().any(|tool| tool.name == "get_editor_snapshot"));
        assert!(!read_only
            .iter()
            .any(|tool| tool.name == "execute_editor_edit"));
        assert!(read_only.iter().any(|tool| tool.name == "attach_psd"));
        assert!(read_only.iter().any(|tool| tool.name == "connect_editor"));
        assert!(read_only
            .iter()
            .any(|tool| tool.name == "get_editor_snapshot"));
        assert!(read_only
            .iter()
            .any(|tool| tool.name == "list_attached_psds"));
        assert!(read_only
            .iter()
            .any(|tool| tool.name == "get_parameter_structure"));
    }

    #[test]
    fn catalog_excludes_agent_only_tools() {
        let names: std::collections::BTreeSet<_> = list_mcp_tools(true)
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        for forbidden in [
            "ask_user",
            "submit_plan",
            "update_plan",
            "read_skill",
            "recall_memory",
            "request_computer_operation",
            "preview_parameter_batch",
        ] {
            assert!(!names.contains(forbidden), "{forbidden} must not be exposed");
        }
    }

    #[test]
    fn mcp_psd_session_is_isolated_from_chat_conversations() {
        assert_eq!(MCP_PSD_SESSION, "mcp");
        assert!(list_mcp_tools(true)
            .iter()
            .any(|tool| tool.name == "attach_psd"));
        assert!(list_mcp_tools(true)
            .iter()
            .any(|tool| tool.name == "read_psd_layer_image"));
    }
}
