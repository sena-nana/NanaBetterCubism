mod edit;
mod read;
mod schema;
mod transaction;
mod verification;

#[cfg(test)]
mod tests;

use self::schema::{field_schema, function_tool, ToolMode, ToolSpec};
use super::{CommandError, EditorService};
use serde_json::{json, Map, Value};
use std::sync::LazyLock;

pub(crate) use read::current_modeling_document;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CurrentModelingDocument {
    pub document_instance_key: String,
    pub document_key: String,
    pub document_path: String,
}

static TOOL_SPECS: LazyLock<Vec<ToolSpec>> =
    LazyLock::new(|| read::specs().into_iter().chain(edit::specs()).collect());

fn tool_specs() -> &'static [ToolSpec] {
    &TOOL_SPECS
}

fn spec(name: &str) -> Option<&'static ToolSpec> {
    tool_specs().iter().find(|spec| spec.tool_name == name)
}

pub(crate) fn tool_display_name(name: &str) -> Option<&'static str> {
    spec(name)
        .map(|spec| spec.display_name)
        .or_else(|| match name {
            "list_editor_notifications" => Some("读取 Editor 通知"),
            "execute_editor_edit" => Some("执行 Editor 修改"),
            "get_editor_edit_result" => Some("查询 Editor 修改结果"),
            "cancel_editor_edit" => Some("取消 Editor 修改"),
            _ => None,
        })
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    let mut tools = tool_specs()
        .iter()
        .map(|spec| {
            let properties = spec
                .fields
                .iter()
                .map(|field| (field.input.to_string(), field_schema(field)))
                .collect::<Map<_, _>>();
            let required = spec
                .fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.input)
                .collect::<Vec<_>>();
            function_tool(
                spec.tool_name,
                spec.description,
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }),
            )
        })
        .collect::<Vec<_>>();
    tools.extend([
        function_tool(
            "list_editor_notifications",
            "读取当前 Editor 连接已收到的官方事件通知。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        function_tool(
            "execute_editor_edit",
            "执行已确认的官方编辑 API 预览；返回 operationId。",
            json!({
                "type": "object",
                "properties": {"previewId": {"type": "string", "minLength": 1}},
                "required": ["previewId"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "get_editor_edit_result",
            "查询官方编辑 API 操作的真实事务与回读结果。",
            json!({
                "type": "object",
                "properties": {"operationId": {"type": "string", "minLength": 1}},
                "required": ["operationId"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "cancel_editor_edit",
            "请求取消正在执行的官方编辑 API 事务。",
            json!({
                "type": "object",
                "properties": {"operationId": {"type": "string", "minLength": 1}},
                "required": ["operationId"],
                "additionalProperties": false
            }),
        ),
    ]);
    tools
}

pub(crate) fn is_tool(name: &str) -> bool {
    name == "list_editor_notifications" || spec(name).is_some()
}

pub(crate) async fn call_tool(
    service: &EditorService,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    if name == "list_editor_notifications" {
        return read::list_notifications(service, args).await;
    }
    let spec = spec(name)
        .ok_or_else(|| CommandError::new("unknown_tool", format!("未知 Editor 工具：{name}")))?;
    match spec.mode {
        ToolMode::Direct => read::execute_direct(service, spec, args).await,
        ToolMode::Preview => edit::preview_edit(service, spec, args).await,
    }
}
