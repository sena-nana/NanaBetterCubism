use crate::agent::capture::capture_cubism_editor_window;
use crate::agent::computer_control::{
    ComputerAction, ComputerActionKind, ComputerOperationOutcome, ComputerOperationStatus,
    ComputerOperationStep, UnsupportedCapability,
};
use crate::agent::memory_recall::MemoryRecallRequest;
use crate::agent::plan::PlanDocument;
use crate::agent::skills;
use crate::agent::store::{truncate_summary, PendingQuestion, PlanStep};
use crate::agent::{new_id, AgentError, AgentRuntime, AgentTurnMode, PendingUserAction};
use crate::domain::{EditorEditOutcome, ParameterBatchInput};
use crate::service::{CommandError, EditorService};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{atomic::AtomicBool, Arc, LazyLock};
use tauri::{AppHandle, Emitter};

struct RegisteredTool {
    schema: Value,
    display_name: String,
    access: ToolAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAccess {
    ReadOnly,
    Mutating,
}

fn all_domain_tool_definitions() -> Vec<RegisteredTool> {
    let mut tools = vec![
        read_tool(
            "get_editor_snapshot",
            "检查 Editor 状态",
            "获取 Cubism Editor 连接状态、能力门控与参数组摘要。",
            json!({"type": "object", "properties": {}}),
        ),
        mutating_tool(
            "connect_editor",
            "连接 Cubism Editor",
            "连接本机 Cubism Editor External API。",
            json!({
                "type": "object",
                "properties": { "port": { "type": "integer", "minimum": 1, "maximum": 65535 } },
                "required": ["port"]
            }),
        ),
        mutating_tool(
            "disconnect_editor",
            "断开 Editor 连接",
            "断开 Cubism Editor 连接。",
            json!({"type": "object", "properties": {}}),
        ),
        read_tool(
            "find_selected_part_parameters",
            "读取选中 Part 参数",
            "查询 Editor 当前选中 Part 子树关联的参数。",
            json!({"type": "object", "properties": {}}),
        ),
        mutating_tool(
            "preview_parameter_batch",
            "预览参数修改",
            "校验并预览批量创建参数，返回 previewId。写操作必须先 preview。",
            json!({
                "type": "object",
                "properties": { "input": { "type": "object" } },
                "required": ["input"]
            }),
        ),
        mutating_tool(
            "execute_parameter_batch",
            "应用参数修改",
            "执行已通过预览的参数批量创建。",
            json!({
                "type": "object",
                "properties": { "previewId": { "type": "string" } },
                "required": ["previewId"]
            }),
        ),
        mutating_tool(
            "cancel_parameter_batch",
            "取消参数修改",
            "取消进行中的参数批量创建事务。",
            json!({
                "type": "object",
                "properties": { "operationId": { "type": "string" } },
                "required": ["operationId"]
            }),
        ),
        read_tool(
            "get_parameter_batch_result",
            "查询参数修改结果",
            "查询参数批量事务的 Rust 终态与回读验证资格。仅 canOfferProjectMemory=true 时可询问是否保存项目记忆。",
            json!({
                "type": "object",
                "properties": { "operationId": { "type": "string" } },
                "required": ["operationId"],
                "additionalProperties": false
            }),
        ),
        read_tool(
            "capture_cubism_editor_window",
            "查看 Editor 窗口",
            "按窗口标题匹配截取 Cubism Editor 窗口。",
            json!({
                "type": "object",
                "properties": {
                    "titleSubstring": { "type": "string", "description": "默认 Cubism Editor" }
                }
            }),
        ),
        read_tool(
            "recall_memory",
            "召回相关记忆",
            "按当前任务语义召回已启用的项目阶段记忆与全局 Live2D 经验；代码自动匹配记忆与分层。",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "当前任务、问题或待查找的记忆主题"
                    },
                    "depth": {
                        "type": "string",
                        "enum": ["index", "focused", "full"],
                        "description": "默认 focused；index 仅摘要，focused 返回命中层，full 返回全部非空层"
                    },
                    "scope": {
                        "type": "string",
                        "enum": ["all", "project", "global"],
                        "description": "默认 all"
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 8,
                        "description": "默认 5"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "upsert_memory",
            "保存记忆",
            "保存或更新一条 Markdown 分层记忆；project 为阶段记忆，global 为 Live2D 经验。提供完整 body，或用 layer+content 单层补丁。",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "minLength": 1 },
                    "expectedRevision": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "更新已有记忆时必填，使用最近一次召回结果中的 revision"
                    },
                    "scope": { "type": "string", "enum": ["project", "global"] },
                    "title": { "type": "string", "minLength": 1 },
                    "body": {
                        "type": "string",
                        "minLength": 1,
                        "description": "完整多层 Markdown；与 layer/content 二选一"
                    },
                    "layer": {
                        "type": "string",
                        "minLength": 1,
                        "description": "单层补丁的层名；须与 content 同用"
                    },
                    "content": {
                        "type": "string",
                        "description": "单层补丁正文；须与 layer 同用"
                    }
                },
                "required": ["scope", "title"],
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "archive_memory",
            "停用记忆",
            "停用一条记忆（enabled=false）。",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "minLength": 1 } },
                "required": ["id"],
                "additionalProperties": false
            }),
        ),
        read_tool(
            "ask_user",
            "等待确认",
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
        read_tool(
            "update_plan",
            "更新计划",
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
        mutating_tool(
            "list_cubism_windows",
            "查找 Cubism 窗口",
            "列出可供电脑代理操作选择的 Cubism 窗口；授权后可列出同进程随后打开的窗口。",
            json!({
                "type": "object",
                "properties": { "grantId": { "type": "string" } },
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "request_computer_operation",
            "请求电脑操作授权",
            "仅当能力矩阵确认官方 API 缺失时，提交完整计划并请求用户授权。",
            json!({
                "type": "object",
                "properties": {
                    "windowId": { "type": "string" },
                    "capability": {
                        "type": "string",
                        "enum": [
                            "art_mesh_geometry", "art_mesh_uv_topology", "warp_control_points",
                            "animation_editing", "physics_editing", "save_export", "texture_atlas",
                            "psd_operations", "glue_creation", "art_path"
                        ]
                    },
                    "goal": { "type": "string" },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" }
                            },
                            "required": ["id", "title"],
                            "additionalProperties": false
                        }
                    },
                    "allowedActions": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": ["click", "double_click", "drag", "scroll", "key", "type_text"]
                        }
                    },
                    "includesFileDialogs": { "type": "boolean" }
                },
                "required": ["windowId", "capability", "goal", "steps", "allowedActions", "includesFileDialogs"],
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "capture_computer_operation_frame",
            "查看最新 Cubism 画面",
            "获取当前授权窗口的最新画面；每个手势前都必须调用。",
            json!({
                "type": "object",
                "properties": {
                    "grantId": { "type": "string" },
                    "windowId": { "type": "string" }
                },
                "required": ["grantId"],
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "perform_computer_action",
            "操作 Cubism 窗口",
            "基于最新 frameId 执行一个已授权手势，并立即返回新画面。",
            json!({
                "type": "object",
                "properties": {
                    "grantId": { "type": "string" },
                    "frameId": { "type": "string" },
                    "stepId": { "type": "string" },
                    "action": { "type": "object" },
                    "settleMs": { "type": "integer", "minimum": 0, "maximum": 2000 }
                },
                "required": ["grantId", "frameId", "stepId", "action"],
                "additionalProperties": false
            }),
        ),
        mutating_tool(
            "finish_computer_operation",
            "结束电脑操作",
            "以真实结果结束电脑代理操作并立即销毁授权。",
            json!({
                "type": "object",
                "properties": {
                    "grantId": { "type": "string" },
                    "outcome": {
                        "type": "string",
                        "enum": ["completed", "needs_user_verification", "partial", "failed", "unknown"]
                    }
                },
                "required": ["grantId", "outcome"],
                "additionalProperties": false
            }),
        ),
    ];
    tools.extend(
        crate::service::official_api::tool_definitions()
            .into_iter()
            .map(|schema| {
                let name = tool_name(&schema).expect("官方 Editor 工具必须有名称");
                let display_name = crate::service::official_api::tool_display_name(name)
                    .expect("官方 Editor 工具必须具有可读名称")
                    .to_string();
                let access = match crate::service::official_api::tool_access(name) {
                    Some(crate::service::official_api::ToolAccess::ReadOnly) => {
                        ToolAccess::ReadOnly
                    }
                    Some(crate::service::official_api::ToolAccess::Mutating) => {
                        ToolAccess::Mutating
                    }
                    None => panic!("官方 Editor 工具必须声明访问属性"),
                };
                RegisteredTool {
                    schema,
                    display_name,
                    access,
                }
            }),
    );
    tools
}

static TOOL_METADATA: LazyLock<BTreeMap<String, (String, ToolAccess)>> = LazyLock::new(|| {
    let mut tools = all_domain_tool_definitions()
        .into_iter()
        .filter_map(|tool| {
            tool_name(&tool.schema)
                .map(|name| (name.to_string(), (tool.display_name, tool.access)))
        })
        .collect::<BTreeMap<_, _>>();
    tools.insert(
        skills::READ_SKILL_TOOL_NAME.into(),
        ("读取任务技能".into(), ToolAccess::ReadOnly),
    );
    tools.insert(
        "submit_plan".into(),
        ("提交计划".into(), ToolAccess::ReadOnly),
    );
    tools
});

pub fn tool_display_name(name: &str) -> Option<&'static str> {
    TOOL_METADATA.get(name).map(|(display_name, _)| display_name.as_str())
}

pub fn tool_access(name: &str) -> Option<ToolAccess> {
    TOOL_METADATA.get(name).map(|(_, access)| *access)
}

pub fn tool_definitions(
    active_skills: &BTreeSet<String>,
    mode: AgentTurnMode,
) -> Result<Vec<Value>, AgentError> {
    let allowed = skills::allowed_domain_tools(active_skills)?;
    let domain_tools = all_domain_tool_definitions();
    let available = domain_tools
        .iter()
        .filter_map(|tool| tool_name(&tool.schema))
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
            .filter(|tool| {
                tool_name(&tool.schema).is_some_and(|name| allowed.contains(name))
                    && (!mode.is_read_only() || tool.access == ToolAccess::ReadOnly)
            })
            .map(|tool| tool.schema),
    );
    if mode == AgentTurnMode::Plan {
        tools.push(submit_plan_tool().schema);
    }
    Ok(tools)
}

#[cfg(test)]
pub fn all_tool_definitions() -> Result<Vec<Value>, AgentError> {
    let domain_tools = all_domain_tool_definitions();
    let available = domain_tools
        .iter()
        .filter_map(|tool| tool_name(&tool.schema))
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
    tools.extend(domain_tools.into_iter().map(|tool| tool.schema));
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

fn registered_tool(
    name: &str,
    display_name: &str,
    description: &str,
    parameters: Value,
    access: ToolAccess,
) -> RegisteredTool {
    RegisteredTool {
        schema: json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": parameters
            }
        }),
        display_name: display_name.into(),
        access,
    }
}

fn read_tool(
    name: &str,
    display_name: &str,
    description: &str,
    parameters: Value,
) -> RegisteredTool {
    registered_tool(
        name,
        display_name,
        description,
        parameters,
        ToolAccess::ReadOnly,
    )
}

fn mutating_tool(
    name: &str,
    display_name: &str,
    description: &str,
    parameters: Value,
) -> RegisteredTool {
    registered_tool(
        name,
        display_name,
        description,
        parameters,
        ToolAccess::Mutating,
    )
}

fn submit_plan_tool() -> RegisteredTool {
    read_tool(
        "submit_plan",
        "提交计划",
        "提交完整结构化计划并等待用户确认。",
        json!({
            "type": "object",
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "summary": {"type": "string", "minLength": 1},
                "steps": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}},
                "diagram": {"type": "string", "minLength": 1},
                "acceptance": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}},
                "assumptions": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}},
                "risks": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}}
            },
            "required": ["title", "summary", "steps", "diagram", "acceptance", "assumptions", "risks"],
            "additionalProperties": false
        }),
    )
}

pub enum ToolOutcome {
    Result {
        content: String,
        image_path: Option<String>,
    },
    AwaitUser {
        action: PendingUserAction,
        tool_call_id: String,
    },
    PlanSubmitted(PlanDocument),
}

pub struct ToolExecutionContext<'a> {
    pub app: &'a AppHandle,
    pub runtime: &'a AgentRuntime,
    pub editor: &'a EditorService,
    pub conversation_id: &'a str,
    pub tool_call_id: &'a str,
    pub cancel: Arc<AtomicBool>,
    pub mode: AgentTurnMode,
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
    ensure_tool_access(context.mode, name)?;
    let app = context.app;
    let runtime = context.runtime;
    let conversation_id = context.conversation_id;
    let tool_call_id = context.tool_call_id;
    emit_tool(app, conversation_id, tool_call_id, name, "started", "");

    let outcome = execute_tool_inner(context, name, arguments).await;
    match &outcome {
        Ok(ToolOutcome::Result { content, .. }) => {
            let summary = if is_computer_tool(name) {
                computer_tool_summary(name, content)
            } else {
                truncate_summary(content, 180)
            };
            emit_tool(
                app,
                conversation_id,
                tool_call_id,
                name,
                "finished",
                &summary,
            );
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                &summary,
                Some(name),
                Some("finished"),
            );
            let (trace_arguments, trace_result) = safe_tool_trace(name, arguments, content);
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                &trace_arguments,
                &trace_result,
                "finished",
            );
        }
        Ok(ToolOutcome::AwaitUser { .. }) => {
            emit_tool(
                app,
                conversation_id,
                tool_call_id,
                name,
                "finished",
                "等待用户回答",
            );
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                "等待用户处理",
                Some(name),
                Some("finished"),
            );
            let (trace_arguments, trace_result) = safe_tool_trace(name, arguments, "waiting_user");
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                &trace_arguments,
                &trace_result,
                "waiting_user",
            );
        }
        Ok(ToolOutcome::PlanSubmitted(_)) => {
            emit_tool(
                app,
                conversation_id,
                tool_call_id,
                name,
                "finished",
                "计划已提交",
            );
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                "structured_plan",
                "awaiting_approval",
                "finished",
            );
        }
        Err(error) => {
            if is_computer_tool(name) {
                if error.code == "input_outcome_unknown" {
                    runtime
                        .computer_control
                        .revoke_grant_for_conversation(conversation_id);
                    emit_computer_status(app, conversation_id, ComputerOperationStatus::Unknown);
                } else if is_terminal_computer_error(&error.code) {
                    emit_computer_status(app, conversation_id, ComputerOperationStatus::Failed);
                }
            }
            emit_tool(
                app,
                conversation_id,
                tool_call_id,
                name,
                "failed",
                &error.message,
            );
            let _ = runtime.store.append_message(
                conversation_id,
                "tool",
                &error.message,
                Some(name),
                Some("failed"),
            );
            let (trace_arguments, trace_result) = safe_tool_trace(name, arguments, &error.code);
            let _ = runtime.store.append_tool_trace(
                conversation_id,
                tool_call_id,
                name,
                &trace_arguments,
                &trace_result,
                "failed",
            );
        }
    }
    outcome
}

fn ensure_tool_access(mode: AgentTurnMode, name: &str) -> Result<(), AgentError> {
    let access = tool_access(name)
        .ok_or_else(|| AgentError::new("unknown_tool", format!("未知工具：{name}")))?;
    if mode.is_read_only() && access == ToolAccess::Mutating {
        return Err(AgentError::new(
            "read_only_mode",
            "当前模式只允许读取，已拒绝写操作。",
        ));
    }
    if name == "submit_plan" && mode != AgentTurnMode::Plan {
        return Err(AgentError::new(
            "tool_not_available",
            "submit_plan 仅在计划模式可用。",
        ));
    }
    Ok(())
}

async fn execute_tool_inner(
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
        mode: _,
    } = context;
    let args: Value = serde_json::from_str(arguments)
        .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;

    let outcome = match name {
        "submit_plan" => {
            let plan: PlanDocument = serde_json::from_value(args)
                .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            Ok(ToolOutcome::PlanSubmitted(plan.validate()?))
        }
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
        "get_parameter_batch_result" => {
            let operation_id = args
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 operationId"))?;
            let result = editor
                .parameter_batch_result(operation_id)
                .await
                .map_err(map_command_error)?;
            let project_bound = runtime
                .store
                .conversation_project_id(conversation_id)?
                .is_some();
            let can_offer = can_offer_project_memory(
                &result.outcome,
                result.outcome == EditorEditOutcome::Committed,
                project_bound,
            );
            Ok(tool_result(result_with_memory_offer(&result, can_offer)?))
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
            let project_bound = runtime
                .store
                .conversation_project_id(conversation_id)?
                .is_some();
            let can_offer = can_offer_project_memory(
                &result.outcome,
                result.verification.is_some(),
                project_bound,
            );
            Ok(tool_result(result_with_memory_offer(&result, can_offer)?))
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
        "recall_memory" => {
            let request: MemoryRecallRequest = serde_json::from_value(args)
                .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let result = runtime
                .store
                .recall_agent_memories(conversation_id, request)?;
            Ok(tool_result(serde_json::to_string_pretty(&result)?))
        }
        "upsert_memory" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string);
            let body = args
                .get("body")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let layer = args
                .get("layer")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let content = args.get("content").and_then(Value::as_str);
            let expected_revision = args.get("expectedRevision").and_then(Value::as_i64);
            let memory = runtime.store.upsert_agent_memory(
                conversation_id,
                id,
                expected_revision,
                required_string(&args, "scope")?,
                required_string(&args, "title")?,
                body,
                layer,
                content,
            )?;
            Ok(tool_result(serde_json::to_string_pretty(&memory)?))
        }
        "archive_memory" => {
            runtime
                .store
                .archive_agent_memory(conversation_id, required_string(&args, "id")?)?;
            Ok(tool_result("已停用记忆。"))
        }
        "list_cubism_windows" => {
            let grant_id = args.get("grantId").and_then(Value::as_str);
            let windows = runtime.computer_control.list_windows(grant_id)?;
            Ok(tool_result(serde_json::to_string_pretty(&windows)?))
        }
        "request_computer_operation" => {
            let window_id = required_string(&args, "windowId")?;
            let capability: UnsupportedCapability = serde_json::from_value(
                args.get("capability")
                    .cloned()
                    .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 capability"))?,
            )
            .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let goal = required_string(&args, "goal")?.to_string();
            let steps: Vec<ComputerOperationStep> =
                serde_json::from_value(args.get("steps").cloned().unwrap_or_else(|| json!([])))
                    .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let allowed_actions: Vec<ComputerActionKind> = serde_json::from_value(
                args.get("allowedActions")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )
            .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let includes_file_dialogs = args
                .get("includesFileDialogs")
                .and_then(Value::as_bool)
                .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 includesFileDialogs"))?;
            let document_instance_key =
                crate::service::official_api::current_modeling_document(editor)
                .await
                .map(|document| document.document_instance_key);
            let approval = runtime.computer_control.request_approval(
                conversation_id,
                window_id,
                capability,
                goal,
                steps,
                allowed_actions,
                includes_file_dialogs,
                document_instance_key,
            )?;
            emit_computer_status(
                app,
                conversation_id,
                ComputerOperationStatus::AwaitingApproval,
            );
            Ok(ToolOutcome::AwaitUser {
                action: approval.into(),
                tool_call_id: tool_call_id.into(),
            })
        }
        "capture_computer_operation_frame" => {
            let grant_id = required_string(&args, "grantId")?;
            let window_id = args.get("windowId").and_then(Value::as_str);
            let cache = runtime
                .store
                .cache_dir()
                .ok_or_else(|| AgentError::new("store_not_ready", "缓存目录不可用。"))?;
            let document_instance_key =
                crate::service::official_api::current_modeling_document(editor)
                .await
                .map(|document| document.document_instance_key);
            let captured = runtime.computer_control.capture_frame(
                conversation_id,
                grant_id,
                window_id,
                &cache,
                document_instance_key.as_deref(),
            )?;
            emit_computer_status(app, conversation_id, ComputerOperationStatus::Running);
            Ok(ToolOutcome::Result {
                content: serde_json::to_string_pretty(&captured.frame)?,
                image_path: Some(captured.path),
            })
        }
        "perform_computer_action" => {
            let grant_id = required_string(&args, "grantId")?;
            let frame_id = required_string(&args, "frameId")?;
            let step_id = required_string(&args, "stepId")?;
            let action: ComputerAction = serde_json::from_value(
                args.get("action")
                    .cloned()
                    .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 action"))?,
            )
            .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let settle_ms = args.get("settleMs").and_then(Value::as_u64).unwrap_or(300);
            let cache = runtime
                .store
                .cache_dir()
                .ok_or_else(|| AgentError::new("store_not_ready", "缓存目录不可用。"))?;
            let document_instance_key =
                crate::service::official_api::current_modeling_document(editor)
                .await
                .map(|document| document.document_instance_key);
            let captured = runtime.computer_control.perform_action(
                conversation_id,
                grant_id,
                frame_id,
                step_id,
                &action,
                settle_ms,
                &cache,
                document_instance_key.as_deref(),
                &cancel,
            )?;
            emit_computer_status(app, conversation_id, ComputerOperationStatus::Running);
            Ok(ToolOutcome::Result {
                content: serde_json::to_string_pretty(&captured.frame)?,
                image_path: Some(captured.path),
            })
        }
        "finish_computer_operation" => {
            let grant_id = required_string(&args, "grantId")?;
            let outcome: ComputerOperationOutcome = serde_json::from_value(
                args.get("outcome")
                    .cloned()
                    .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 outcome"))?,
            )
            .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
            let result = runtime
                .computer_control
                .finish(conversation_id, grant_id, outcome)?;
            let status = match outcome {
                ComputerOperationOutcome::Completed => ComputerOperationStatus::Completed,
                ComputerOperationOutcome::NeedsUserVerification
                | ComputerOperationOutcome::Partial => {
                    ComputerOperationStatus::NeedsUserVerification
                }
                ComputerOperationOutcome::Failed => ComputerOperationStatus::Failed,
                ComputerOperationOutcome::Unknown => ComputerOperationStatus::Unknown,
            };
            emit_computer_status(app, conversation_id, status);
            Ok(tool_result(serde_json::to_string_pretty(&result)?))
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
            let question = PendingQuestion {
                action_id: new_id(),
                conversation_id: conversation_id.into(),
                question,
                options,
            };
            runtime
                .store
                .set_pending_question(&question, tool_call_id)?;
            Ok(ToolOutcome::AwaitUser {
                action: question.into(),
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

    outcome
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, AgentError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AgentError::new("invalid_arguments", format!("缺少 {key}")))
}

fn can_offer_project_memory(
    outcome: &EditorEditOutcome,
    reread_verified: bool,
    project_bound: bool,
) -> bool {
    *outcome == EditorEditOutcome::Committed && reread_verified && project_bound
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultWithMemoryOffer<'a, T> {
    #[serde(flatten)]
    result: &'a T,
    can_offer_project_memory: bool,
}

fn result_with_memory_offer(
    result: &impl Serialize,
    can_offer_project_memory: bool,
) -> Result<String, AgentError> {
    Ok(serde_json::to_string_pretty(&ResultWithMemoryOffer {
        result,
        can_offer_project_memory,
    })?)
}

pub(crate) fn is_computer_tool(name: &str) -> bool {
    matches!(
        name,
        "list_cubism_windows"
            | "request_computer_operation"
            | "capture_computer_operation_frame"
            | "perform_computer_action"
            | "finish_computer_operation"
    )
}

fn is_terminal_computer_error(code: &str) -> bool {
    matches!(
        code,
        "grant_not_found"
            | "approval_expired"
            | "document_changed"
            | "computer_action_limit"
            | "plan_changed"
            | "action_not_approved"
            | "window_not_approved"
            | "process_changed"
            | "stale_window"
    )
}

fn safe_tool_trace(name: &str, arguments: &str, result: &str) -> (String, String) {
    if !is_computer_tool(name) {
        return (arguments.to_string(), result.to_string());
    }
    let parsed: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    let safe_arguments = match name {
        "request_computer_operation" => json!({
            "capability": parsed.get("capability"),
            "stepCount": parsed.get("steps").and_then(Value::as_array).map(Vec::len),
            "allowedActions": parsed.get("allowedActions"),
            "includesFileDialogs": parsed.get("includesFileDialogs"),
        }),
        "perform_computer_action" => json!({
            "stepId": parsed.get("stepId"),
            "actionType": parsed.pointer("/action/kind"),
        }),
        "finish_computer_operation" => json!({ "outcome": parsed.get("outcome") }),
        "list_cubism_windows" => {
            json!({ "phase": if parsed.get("grantId").is_some() { "authorized" } else { "selection" } })
        }
        _ => json!({ "action": "capture" }),
    };
    let parsed_result = serde_json::from_str::<Value>(result).unwrap_or_else(|_| json!({}));
    let safe_result = match name {
        "list_cubism_windows" => {
            json!({ "windowCount": parsed_result.as_array().map(Vec::len) })
        }
        "perform_computer_action" => json!({ "actionCount": 1, "result": "captured" }),
        "finish_computer_operation" => json!({
            "actionCount": parsed_result.get("actionCount"),
            "outcome": parsed_result.get("outcome"),
        }),
        "capture_computer_operation_frame" => json!({ "result": "captured" }),
        _ => json!({ "recorded": true }),
    };
    (safe_arguments.to_string(), safe_result.to_string())
}

fn computer_tool_summary(name: &str, result: &str) -> String {
    match name {
        "list_cubism_windows" => serde_json::from_str::<Value>(result)
            .ok()
            .and_then(|value| value.as_array().map(Vec::len))
            .map(|count| format!("发现 {count} 个 Cubism 窗口"))
            .unwrap_or_else(|| "已检查 Cubism 窗口".into()),
        "request_computer_operation" => "等待用户授权电脑代理操作".into(),
        "capture_computer_operation_frame" => "已获取最新 Cubism 画面".into(),
        "perform_computer_action" => "已执行一个授权手势并获取新画面".into(),
        "finish_computer_operation" => "电脑代理操作已结束".into(),
        _ => "电脑代理操作状态已更新".into(),
    }
}

fn emit_computer_status(app: &AppHandle, conversation_id: &str, status: ComputerOperationStatus) {
    let _ = app.emit(
        "agent://computer-operation",
        json!({ "conversationId": conversation_id, "status": status }),
    );
}

pub(crate) fn emit_tool(
    app: &AppHandle,
    conversation_id: &str,
    tool_call_id: &str,
    tool_name: &str,
    status: &str,
    summary: &str,
) {
    let _ = app.emit(
        "agent://tool",
        json!({
            "conversationId": conversation_id,
            "toolCallId": tool_call_id,
            "toolName": tool_name,
            "toolDisplayName": tool_display_name(tool_name).unwrap_or("未知工具"),
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
        let definitions = tool_definitions(&BTreeSet::new(), AgentTurnMode::Default).unwrap();
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
        let parameter = tool_definitions(
            &BTreeSet::from(["parameter-editing".into()]),
            AgentTurnMode::Default,
        )
        .unwrap();
        let parameter_names = names(&parameter);
        assert!(parameter_names.contains("preview_add_parameter"));
        assert!(parameter_names.contains("execute_editor_edit"));
        assert!(parameter_names.contains("get_parameter_batch_result"));
        assert!(!parameter_names.contains("preview_add_part"));
        assert!(!parameter_names.contains("recall_memory"));

        let object = tool_definitions(
            &BTreeSet::from(["object-editing".into()]),
            AgentTurnMode::Default,
        )
        .unwrap();
        assert!(!names(&object).contains("get_parameter_batch_result"));

        let combined = tool_definitions(
            &BTreeSet::from(["parameter-editing".into(), "object-editing".into()]),
            AgentTurnMode::Default,
        )
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
    fn memory_skills_expose_separate_strict_contracts() {
        let recall_definitions = tool_definitions(
            &BTreeSet::from(["memory-recall".into()]),
            AgentTurnMode::Default,
        )
        .unwrap();
        let recall_names = names(&recall_definitions);
        assert!(recall_names.contains("recall_memory"));
        assert!(!recall_names.contains("upsert_memory"));
        assert!(!recall_names.contains("archive_memory"));
        assert!(!recall_names.contains("list_memories"));
        assert!(!recall_names.contains("read_memory"));
        let recall = recall_definitions
            .iter()
            .find(|definition| tool_name(definition) == Some("recall_memory"))
            .unwrap();
        let recall_parameters = &recall["function"]["parameters"];
        assert_eq!(recall_parameters["required"], json!(["query"]));
        assert_eq!(recall_parameters["additionalProperties"], false);
        assert_eq!(
            recall_parameters["properties"]["depth"]["enum"],
            json!(["index", "focused", "full"])
        );
        assert_eq!(recall_parameters["properties"]["limit"]["maximum"], 8);

        let definitions = tool_definitions(
            &BTreeSet::from(["project-memory".into()]),
            AgentTurnMode::Default,
        )
        .unwrap();
        let memory_names = names(&definitions);
        assert!(memory_names.contains("upsert_memory"));
        assert!(memory_names.contains("archive_memory"));
        assert!(!memory_names.contains("recall_memory"));

        let upsert = definitions
            .iter()
            .find(|definition| tool_name(definition) == Some("upsert_memory"))
            .unwrap();
        let parameters = &upsert["function"]["parameters"];
        assert_eq!(parameters["additionalProperties"], false);
        assert_eq!(
            parameters["properties"]["scope"]["enum"],
            json!(["project", "global"])
        );
        assert_eq!(parameters["required"], json!(["scope", "title"]));
        assert!(parameters["properties"].get("body").is_some());
        assert!(parameters["properties"].get("expectedRevision").is_some());
        assert!(parameters["properties"].get("layer").is_some());
        assert!(parameters["properties"].get("content").is_some());
        assert!(parameters["properties"].get("kind").is_none());
        assert!(parameters["properties"].get("enabled").is_none());

        let archive = definitions
            .iter()
            .find(|definition| tool_name(definition) == Some("archive_memory"))
            .unwrap();
        assert_eq!(
            archive["function"]["parameters"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn every_domain_tool_is_core_or_assigned_to_a_skill() {
        let definitions = all_tool_definitions().unwrap();
        let registered_names = names(&definitions);
        assert_eq!(registered_names.len(), definitions.len());
        assert!(registered_names.contains("read_skill"));
        assert_eq!(tool_display_name("read_skill"), Some("读取任务技能"));
        assert!(registered_names.iter().all(|name| {
            tool_display_name(name).is_some_and(|display_name| !display_name.trim().is_empty())
        }));
        for forbidden in ["read_file", "write_file", "apply_patch", "run_terminal"] {
            assert!(!registered_names.contains(forbidden));
        }
    }

    #[test]
    fn read_only_modes_never_advertise_mutating_tools() {
        let active = skills::all()
            .unwrap()
            .iter()
            .map(|skill| skill.name.clone())
            .collect::<BTreeSet<_>>();
        for mode in [AgentTurnMode::ConversationOnly, AgentTurnMode::Plan] {
            let definitions = tool_definitions(&active, mode).unwrap();
            assert!(names(&definitions)
                .iter()
                .all(|name| tool_access(name) == Some(ToolAccess::ReadOnly)));
        }
        assert!(
            names(&tool_definitions(&active, AgentTurnMode::Plan).unwrap()).contains("submit_plan")
        );
        assert!(
            !names(&tool_definitions(&active, AgentTurnMode::ConversationOnly).unwrap())
                .contains("submit_plan")
        );
    }

    #[test]
    fn execution_guard_rejects_directly_constructed_writes() {
        for mode in [AgentTurnMode::ConversationOnly, AgentTurnMode::Plan] {
            for name in [
                "connect_editor",
                "preview_parameter_batch",
                "execute_editor_edit",
                "set_parameter_values",
                "perform_computer_action",
                "capture_computer_operation_frame",
                "list_cubism_windows",
                "upsert_memory",
            ] {
                assert!(matches!(
                    ensure_tool_access(mode, name),
                    Err(error) if error.code == "read_only_mode"
                ));
            }
            assert!(ensure_tool_access(mode, "get_parameter_values").is_ok());
        }
        assert!(ensure_tool_access(AgentTurnMode::Default, "submit_plan").is_err());
        assert!(ensure_tool_access(AgentTurnMode::Plan, "submit_plan").is_ok());
    }

    #[test]
    fn computer_tool_traces_drop_sensitive_values() {
        let (arguments, result) = safe_tool_trace(
            "perform_computer_action",
            r#"{"grantId":"secret","frameId":"frame","stepId":"move","action":{"kind":"type_text","text":"C:\\private\\model.cmo3","x":42}}"#,
            r#"{"frameId":"next","path":"C:\\cache\\capture.png"}"#,
        );
        assert!(arguments.contains("type_text"));
        assert!(arguments.contains("move"));
        for sensitive in [
            "secret",
            "frame",
            "private",
            "model.cmo3",
            "42",
            "capture.png",
        ] {
            assert!(!arguments.contains(sensitive));
            assert!(!result.contains(sensitive));
        }
    }

    #[test]
    fn project_memory_offer_requires_verified_commit_and_bound_project() {
        let cases = [
            (EditorEditOutcome::Committed, true, true, true),
            (EditorEditOutcome::Committed, false, true, false),
            (EditorEditOutcome::Committed, true, false, false),
            (EditorEditOutcome::Running, true, true, false),
            (EditorEditOutcome::CancelledRolledBack, true, true, false),
            (EditorEditOutcome::FailedRolledBack, true, true, false),
            (EditorEditOutcome::Failed, true, true, false),
            (EditorEditOutcome::Unknown, true, true, false),
        ];
        for (outcome, verified, project_bound, expected) in cases {
            assert_eq!(
                can_offer_project_memory(&outcome, verified, project_bound),
                expected,
                "unexpected eligibility for {outcome:?}",
            );
        }
    }
}
