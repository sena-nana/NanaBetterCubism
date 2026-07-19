use crate::agent::AgentError;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::LazyLock;

pub const READ_SKILL_TOOL_NAME: &str = "read_skill";
pub const MAX_SKILL_LOAD_STEPS: usize = 7;

const CORE_DOMAIN_TOOLS: &[&str] = &[
    "get_editor_snapshot",
    "connect_editor",
    "disconnect_editor",
    "ask_user",
    "update_plan",
];

const EDITOR_CONTEXT_TOOLS: &[&str] = &[
    "capture_cubism_editor_window",
    "list_editor_documents",
    "get_editor_document",
    "get_current_document",
    "get_current_model",
    "get_current_edit_mode",
    "get_physics_info",
    "send_cubism_log",
    "notify_physics_file_exported",
    "notify_moc_file_exported",
    "notify_motion_file_exported",
    "notify_motion_sync_file_exported",
    "notify_change_edit_mode",
    "list_editor_notifications",
];

const MODEL_INSPECTION_TOOLS: &[&str] = &[
    "find_selected_part_parameters",
    "get_parameter_values",
    "get_parameters",
    "get_parameter_groups",
    "get_parameter_keys",
    "get_objects_by_parameter_key",
    "get_parameter_structure",
    "get_selected_objects",
    "get_part_structure",
    "get_object",
    "get_deformer_structure",
];

const PARAMETER_EDITING_TOOLS: &[&str] = &[
    "set_parameter_values",
    "clear_parameter_values",
    "preview_parameter_batch",
    "execute_parameter_batch",
    "get_parameter_batch_result",
    "cancel_parameter_batch",
    "preview_add_parameter_key",
    "preview_delete_parameter_key",
    "preview_move_parameter_key",
    "preview_add_parameter",
    "preview_add_parameter_group",
    "preview_edit_parameter",
    "preview_edit_parameter_group",
    "preview_delete_parameter",
    "preview_delete_parameter_group",
    "preview_move_parameter",
    "preview_move_parameter_group",
    "execute_editor_edit",
    "get_editor_edit_result",
    "cancel_editor_edit",
];

const OBJECT_EDITING_TOOLS: &[&str] = &[
    "preview_add_selected_objects",
    "preview_clear_selected_objects",
    "preview_delete_object",
    "preview_move_object_on_parts_palette",
    "preview_add_part",
    "preview_edit_part",
    "preview_edit_art_mesh",
    "preview_edit_glue",
    "preview_add_rotation_deformer",
    "preview_add_warp_deformer",
    "preview_edit_rotation_deformer",
    "preview_edit_warp_deformer",
    "execute_editor_edit",
    "get_editor_edit_result",
    "cancel_editor_edit",
];

const MEMORY_RECALL_TOOLS: &[&str] = &["recall_memory"];

const PROJECT_MEMORY_TOOLS: &[&str] = &["upsert_memory", "archive_memory"];

const COMPUTER_OPERATION_TOOLS: &[&str] = &[
    "list_cubism_windows",
    "request_computer_operation",
    "capture_computer_operation_frame",
    "perform_computer_action",
    "finish_computer_operation",
];

struct SkillSource {
    content: &'static str,
    tools: &'static [&'static str],
}

#[derive(Debug)]
pub struct RuntimeSkill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    tools: &'static [&'static str],
}

static SKILLS: LazyLock<Result<Vec<RuntimeSkill>, AgentError>> = LazyLock::new(|| {
    [
        SkillSource {
            content: include_str!("skills/editor-context/SKILL.md"),
            tools: EDITOR_CONTEXT_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/model-inspection/SKILL.md"),
            tools: MODEL_INSPECTION_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/parameter-editing/SKILL.md"),
            tools: PARAMETER_EDITING_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/object-editing/SKILL.md"),
            tools: OBJECT_EDITING_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/memory-recall/SKILL.md"),
            tools: MEMORY_RECALL_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/project-memory/SKILL.md"),
            tools: PROJECT_MEMORY_TOOLS,
        },
        SkillSource {
            content: include_str!("skills/computer-operation/SKILL.md"),
            tools: COMPUTER_OPERATION_TOOLS,
        },
    ]
    .into_iter()
    .map(parse_skill)
    .collect()
});

fn parse_skill(source: SkillSource) -> Result<RuntimeSkill, AgentError> {
    let mut lines = source.content.lines();
    if lines.next() != Some("---") {
        return Err(AgentError::new(
            "invalid_skill",
            "运行时 SKILL 缺少 frontmatter。",
        ));
    }

    let mut name = None;
    let mut description = None;
    loop {
        let line = lines
            .next()
            .ok_or_else(|| AgentError::new("invalid_skill", "SKILL frontmatter 未结束。"))?;
        if line == "---" {
            break;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| AgentError::new("invalid_skill", "SKILL frontmatter 格式无效。"))?;
        let value = value.trim();
        match key.trim() {
            "name" if name.is_none() && !value.is_empty() => name = Some(value.to_string()),
            "description" if description.is_none() && !value.is_empty() => {
                description = Some(value.to_string())
            }
            _ => {
                return Err(AgentError::new(
                    "invalid_skill",
                    "SKILL frontmatter 只允许 name 和 description。",
                ))
            }
        }
    }

    let instructions = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    if instructions.is_empty() {
        return Err(AgentError::new("invalid_skill", "SKILL 正文不能为空。"));
    }

    Ok(RuntimeSkill {
        name: name.ok_or_else(|| AgentError::new("invalid_skill", "SKILL 缺少 name。"))?,
        description: description
            .ok_or_else(|| AgentError::new("invalid_skill", "SKILL 缺少 description。"))?,
        instructions,
        tools: source.tools,
    })
}

pub fn all() -> Result<&'static [RuntimeSkill], AgentError> {
    SKILLS.as_ref().map(Vec::as_slice).map_err(Clone::clone)
}

pub fn get(name: &str) -> Result<&'static RuntimeSkill, AgentError> {
    all()?
        .iter()
        .find(|skill| skill.name == name)
        .ok_or_else(|| AgentError::new("unknown_skill", format!("未知运行时 SKILL：{name}")))
}

pub fn catalog_prompt() -> Result<String, AgentError> {
    let entries = all()?
        .iter()
        .map(|skill| format!("- {}: {}", skill.name, skill.description))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "可用运行时 SKILL：\n{entries}\n需要领域能力时，先调用 read_skill 读取最少数量的相关 SKILL；不要预先读取全部 SKILL。"
    ))
}

pub fn read_skill_tool_definition() -> Result<Value, AgentError> {
    let names = all()?
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<Vec<_>>();
    Ok(json!({
        "type": "function",
        "function": {
            "name": READ_SKILL_TOOL_NAME,
            "description": "读取一个与当前任务相关的运行时 SKILL，并从下一次模型请求开始开放其工具。",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "enum": names }
                },
                "required": ["name"],
                "additionalProperties": false
            }
        }
    }))
}

pub fn parse_read_arguments(arguments: &str) -> Result<&'static RuntimeSkill, AgentError> {
    let value: Value = serde_json::from_str(arguments)
        .map_err(|error| AgentError::new("invalid_arguments", error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AgentError::new("invalid_arguments", "SKILL 参数必须是对象。"))?;
    if object.len() != 1 {
        return Err(AgentError::new(
            "invalid_arguments",
            "read_skill 只接受 name。",
        ));
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AgentError::new("invalid_arguments", "缺少 SKILL name。"))?;
    get(name)
}

pub fn allowed_domain_tools(
    active_skills: &BTreeSet<String>,
) -> Result<BTreeSet<&'static str>, AgentError> {
    let mut allowed = CORE_DOMAIN_TOOLS.iter().copied().collect::<BTreeSet<_>>();
    for name in active_skills {
        allowed.extend(get(name)?.tools.iter().copied());
    }
    Ok(allowed)
}

#[cfg(test)]
pub fn all_declared_domain_tools() -> Result<BTreeSet<&'static str>, AgentError> {
    let mut tools = CORE_DOMAIN_TOOLS.iter().copied().collect::<BTreeSet<_>>();
    for skill in all()? {
        tools.extend(skill.tools.iter().copied());
    }
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_skills_have_valid_unique_metadata() {
        let skills = all().unwrap();
        assert_eq!(skills.len(), MAX_SKILL_LOAD_STEPS);
        let names = skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), skills.len());
        assert!(skills
            .iter()
            .all(|skill| !skill.description.is_empty() && !skill.instructions.is_empty()));
    }

    #[test]
    fn read_skill_rejects_unknown_or_extra_arguments() {
        assert!(matches!(
            parse_read_arguments(r#"{"name":"missing"}"#),
            Err(error) if error.code == "unknown_skill"
        ));
        assert!(matches!(
            parse_read_arguments(r#"{"name":"editor-context","extra":true}"#),
            Err(error) if error.code == "invalid_arguments"
        ));
    }
}
