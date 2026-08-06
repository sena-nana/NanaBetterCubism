mod edit;
mod read;
mod schema;
mod transaction;
mod verification;

#[cfg(test)]
mod tests;

pub(crate) use self::schema::ToolAccess;
use self::schema::{field_schema, function_tool, ToolMode, ToolSpec};
use super::{CommandError, EditorService};
use crate::domain::MAX_BATCH_SIZE;
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
    spec(name).map(|spec| spec.display_name).or(match name {
        "list_editor_notifications" => Some("读取 Editor 通知"),
        "get_objects" => Some("批量读取对象属性"),
        "get_all_objects" => Some("全量读取对象属性"),
        "execute_editor_edit" => Some("执行 Editor 修改"),
        "get_editor_edit_result" => Some("查询 Editor 修改结果"),
        "cancel_editor_edit" => Some("取消 Editor 修改"),
        _ => None,
    })
}

pub(crate) fn tool_access(name: &str) -> Option<ToolAccess> {
    spec(name).map(|spec| spec.access).or(match name {
        "list_editor_notifications"
        | "get_objects"
        | "get_all_objects"
        | "get_editor_edit_result" => Some(ToolAccess::ReadOnly),
        "execute_editor_edit" | "cancel_editor_edit" => Some(ToolAccess::Mutating),
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
            let parameters = if spec.mode == ToolMode::Preview {
                json!({
                    "type": "object",
                    "properties": {
                        "operations": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": MAX_BATCH_SIZE,
                            "items": {
                                "type": "object",
                                "properties": properties,
                                "required": required,
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["operations"],
                    "additionalProperties": false
                })
            } else {
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                })
            };
            let description = if spec.mode == ToolMode::Preview {
                format!(
                    "{} 使用 operations 传入 1 到 {MAX_BATCH_SIZE} 个同类型操作，并生成一个事务预览。",
                    spec.description
                )
            } else {
                spec.description.into()
            };
            function_tool(spec.tool_name, &description, parameters)
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
            "get_objects",
            "按稳定 ID 批量读取 Part、ArtMesh、Glue 或 Deformer 属性（含 draw order、opacity 等）。ids 使用结构读取返回的精确值，1 到 200 项；不要逐个调用 get_object。",
            read::get_objects_parameters_schema(),
        ),
        function_tool(
            "get_all_objects",
            "读取当前模型全部可 GetObject 的对象属性（含 draw order、opacity 等）。可选 types 过滤对象类型；可选 parameters 作为关键点过滤。",
            read::get_all_objects_parameters_schema(),
        ),
        function_tool(
            "execute_editor_edit",
            "执行已确认的同类型批量编辑预览；整个预览只使用一次 Editor 事务，并返回 operationId。",
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
    matches!(
        name,
        "list_editor_notifications" | "get_objects" | "get_all_objects"
    ) || spec(name).is_some()
}

pub(crate) fn is_preview_tool(name: &str) -> bool {
    name == "preview_parameter_batch"
        || spec(name).is_some_and(|spec| spec.mode == ToolMode::Preview)
}

pub(crate) async fn call_tool(
    service: &EditorService,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    match name {
        "list_editor_notifications" => return read::list_notifications(service, args).await,
        "get_objects" => return read::get_objects(service, args).await,
        "get_all_objects" => return read::get_all_objects(service, args).await,
        _ => {}
    }
    let spec = spec(name)
        .ok_or_else(|| CommandError::new("unknown_tool", format!("未知 Editor 工具：{name}")))?;
    match spec.mode {
        ToolMode::Direct => read::execute_direct(service, spec, args).await,
        ToolMode::Preview => edit::preview_edit(service, spec, args).await,
    }
}
