use crate::agent::AgentError;
use std::collections::BTreeMap;

pub const PROJECT_LAYERS: &[&str] = &["Overview", "Stage", "Structure", "Decisions"];
pub const GLOBAL_LAYERS: &[&str] = &["Summary", "Technique", "Caveats"];

pub fn layers_for_scope(scope: &str) -> Result<&'static [&'static str], AgentError> {
    match scope {
        "project" => Ok(PROJECT_LAYERS),
        "global" => Ok(GLOBAL_LAYERS),
        _ => Err(AgentError::new("invalid_memory", "记忆范围无效。")),
    }
}

pub fn index_layer_name(scope: &str) -> Result<&'static str, AgentError> {
    Ok(layers_for_scope(scope)?[0])
}

pub fn extract_overview(scope: &str, body: &str) -> Result<String, AgentError> {
    let index = index_layer_name(scope)?;
    let sections = parse_sections(body)?;
    Ok(sections
        .get(index)
        .map(|value| value.trim().to_string())
        .unwrap_or_default())
}

pub fn validate_and_normalize(scope: &str, title: &str, body: &str) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    let mut sections = parse_sections(body)?;
    reject_unknown_layers(layers, &sections)?;
    for layer in layers {
        sections.entry((*layer).into()).or_default();
    }
    let index = layers[0];
    if sections.get(index).map(|value| value.trim().is_empty()).unwrap_or(true) {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 的 ## {index} 不能为空。"),
        ));
    }
    Ok(render_body(title, layers, &sections))
}

pub fn patch_layer(
    scope: &str,
    title: &str,
    existing_body: Option<&str>,
    layer: &str,
    content: &str,
) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    if !layers.iter().any(|name| *name == layer) {
        return Err(AgentError::new(
            "invalid_memory_layer",
            format!("记忆层无效：{layer}"),
        ));
    }
    let mut sections = match existing_body {
        Some(body) if !body.trim().is_empty() => {
            let parsed = parse_sections(body)?;
            reject_unknown_layers(layers, &parsed)?;
            parsed
        }
        _ => BTreeMap::new(),
    };
    for name in layers {
        sections.entry((*name).into()).or_default();
    }
    sections.insert(layer.into(), content.trim().to_string());
    let index = layers[0];
    if sections.get(index).map(|value| value.trim().is_empty()).unwrap_or(true) {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 的 ## {index} 不能为空。"),
        ));
    }
    Ok(render_body(title, layers, &sections))
}

pub fn select_layers(
    scope: &str,
    body: &str,
    requested: Option<&[String]>,
) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    let sections = parse_sections(body)?;
    let selected: Vec<&str> = match requested {
        None | Some([]) => vec![layers[0]],
        Some(names) => {
            let mut ordered = Vec::new();
            for name in names {
                if !layers.iter().any(|layer| *layer == name.as_str()) {
                    return Err(AgentError::new(
                        "invalid_memory_layer",
                        format!("记忆层无效：{name}"),
                    ));
                }
                if !ordered.contains(&name.as_str()) {
                    ordered.push(name.as_str());
                }
            }
            ordered
        }
    };
    let mut output = String::new();
    for layer in selected {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str("## ");
        output.push_str(layer);
        let content = sections.get(layer).map(String::as_str).unwrap_or("").trim();
        if !content.is_empty() {
            output.push('\n');
            output.push_str(content);
        }
    }
    Ok(output)
}

fn parse_sections(body: &str) -> Result<BTreeMap<String, String>, AgentError> {
    let mut sections = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut buffer = String::new();

    for line in body.lines() {
        if let Some(heading) = parse_h2(line) {
            if let Some(name) = current.take() {
                sections.insert(name, trim_section(&buffer));
                buffer.clear();
            }
            current = Some(heading);
            continue;
        }
        if line.starts_with("# ") && current.is_none() {
            continue;
        }
        if current.is_some() {
            if !buffer.is_empty() {
                buffer.push('\n');
            }
            buffer.push_str(line);
        }
    }

    if let Some(name) = current {
        sections.insert(name, trim_section(&buffer));
    }

    if sections.is_empty() && !body.trim().is_empty() {
        return Err(AgentError::new(
            "invalid_memory_body",
            "记忆正文必须使用约定的 ## 分层 Markdown。",
        ));
    }
    Ok(sections)
}

fn parse_h2(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("## ")?;
    if rest.starts_with('#') {
        return None;
    }
    let name = rest.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn reject_unknown_layers(
    allowed: &[&str],
    sections: &BTreeMap<String, String>,
) -> Result<(), AgentError> {
    for key in sections.keys() {
        if !allowed.iter().any(|layer| *layer == key.as_str()) {
            return Err(AgentError::new(
                "invalid_memory_body",
                format!("记忆 Markdown 含有未知分层：{key}"),
            ));
        }
    }
    Ok(())
}

fn render_body(title: &str, layers: &[&str], sections: &BTreeMap<String, String>) -> String {
    let mut output = format!("# {}\n", title.trim());
    for layer in layers {
        output.push('\n');
        output.push_str("## ");
        output.push_str(layer);
        output.push('\n');
        if let Some(content) = sections.get(*layer) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push('\n');
            }
        }
    }
    output
}

fn trim_section(value: &str) -> String {
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_project_markdown_and_extracts_overview() {
        let body = validate_and_normalize(
            "project",
            "眼睛参数",
            "## Overview\n已完成眼睛参数结构。\n## Stage\nParamAngleX 已对齐。\n",
        )
        .unwrap();
        assert!(body.contains("# 眼睛参数"));
        assert!(body.contains("## Structure\n"));
        assert!(body.contains("## Decisions\n"));
        assert_eq!(
            extract_overview("project", &body).unwrap(),
            "已完成眼睛参数结构。"
        );
    }

    #[test]
    fn rejects_unknown_layer_and_empty_index() {
        assert_eq!(
            validate_and_normalize("global", "经验", "## Summary\n可用\n## Extra\n否")
                .unwrap_err()
                .code,
            "invalid_memory_body"
        );
        assert_eq!(
            validate_and_normalize("global", "经验", "## Summary\n\n## Technique\nx")
                .unwrap_err()
                .code,
            "invalid_memory_body"
        );
    }

    #[test]
    fn patches_single_layer_and_selects_requested_layers() {
        let body = patch_layer("global", "命名", None, "Summary", "先核对参数 ID。").unwrap();
        let patched = patch_layer(
            "global",
            "命名",
            Some(&body),
            "Technique",
            "列出已有 ID 再新建。",
        )
        .unwrap();
        let selected = select_layers(
            "global",
            &patched,
            Some(&["Summary".into(), "Technique".into()]),
        )
        .unwrap();
        assert!(selected.contains("## Summary\n先核对参数 ID。"));
        assert!(selected.contains("## Technique\n列出已有 ID 再新建。"));
        assert!(!selected.contains("## Caveats"));
    }
}
