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

pub fn strict_layers(scope: &str, body: &str) -> Result<Vec<(String, String)>, AgentError> {
    let layers = layers_for_scope(scope)?;
    let sections = parse_sections(body)?;
    reject_unknown_layers(layers, &sections)?;
    let index = layers[0];
    if sections
        .get(index)
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 的 ## {index} 不能为空。"),
        ));
    }
    Ok(layers
        .iter()
        .map(|layer| {
            (
                (*layer).to_string(),
                sections
                    .get(*layer)
                    .map(|content| content.trim().to_string())
                    .unwrap_or_default(),
            )
        })
        .collect())
}

pub fn layers_for_display(scope: &str, body: &str) -> Result<Vec<(String, String)>, AgentError> {
    let layers = layers_for_scope(scope)?;
    let sections = parse_sections(body)
        .ok()
        .filter(|sections| reject_unknown_layers(layers, sections).is_ok());
    let legacy_body = body.trim();

    Ok(layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let content = sections
                .as_ref()
                .and_then(|sections| sections.get(*layer))
                .map(|content| content.trim().to_string())
                .unwrap_or_else(|| {
                    if sections.is_none() && index == 0 {
                        legacy_body.to_string()
                    } else {
                        String::new()
                    }
                });
            ((*layer).to_string(), content)
        })
        .collect())
}

pub fn validate_and_normalize(scope: &str, title: &str, body: &str) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    let mut sections = parse_sections(body)?;
    reject_unknown_layers(layers, &sections)?;
    for layer in layers {
        sections.entry((*layer).into()).or_default();
    }
    let index = layers[0];
    if sections
        .get(index)
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 的 ## {index} 不能为空。"),
        ));
    }
    Ok(render_body(title, layers, &sections))
}

pub fn normalize_legacy_body(scope: &str, title: &str, body: &str) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    let content = demote_legacy_headings(body);
    if content.trim().is_empty() {
        return Err(AgentError::new(
            "invalid_memory_body",
            "旧版记忆正文不能为空。",
        ));
    }
    let mut sections = BTreeMap::new();
    sections.insert(layers[0].to_string(), content.trim().to_string());
    for layer in &layers[1..] {
        sections.insert((*layer).to_string(), String::new());
    }
    Ok(render_body(title, layers, &sections))
}

pub fn normalize_for_migration(
    scope: &str,
    title: &str,
    body: &str,
) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    match validate_and_normalize(scope, title, body) {
        Ok(normalized) => Ok(normalized),
        Err(error) if has_recognized_layer_heading(layers, body) => Err(error),
        Err(_) => normalize_legacy_body(scope, title, body),
    }
}

pub fn patch_layer(
    scope: &str,
    title: &str,
    existing_body: Option<&str>,
    layer: &str,
    content: &str,
) -> Result<String, AgentError> {
    let layers = layers_for_scope(scope)?;
    if !layers.contains(&layer) {
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
    if sections
        .get(index)
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 的 ## {index} 不能为空。"),
        ));
    }
    Ok(render_body(title, layers, &sections))
}

#[cfg(test)]
pub fn select_layers(
    scope: &str,
    body: &str,
    requested: Option<&[String]>,
) -> Result<String, AgentError> {
    let layers = strict_layers(scope, body)?;
    let selected = match requested {
        None | Some([]) => vec![&layers[0]],
        Some(names) => {
            let mut selected: Vec<&(String, String)> = Vec::new();
            for name in names {
                let layer = layers
                    .iter()
                    .find(|(layer, _)| layer == name)
                    .ok_or_else(|| {
                        AgentError::new("invalid_memory_layer", format!("记忆层无效：{name}"))
                    })?;
                if !selected.iter().any(|(layer_name, _)| layer_name == name) {
                    selected.push(layer);
                }
            }
            selected
        }
    };
    Ok(selected
        .into_iter()
        .map(|(name, content)| {
            if content.is_empty() {
                format!("## {name}")
            } else {
                format!("## {name}\n{content}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n"))
}

fn parse_sections(body: &str) -> Result<BTreeMap<String, String>, AgentError> {
    let mut sections = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut buffer = String::new();
    let mut saw_title = false;
    let mut fence: Option<Fence> = None;

    for line in body.lines() {
        if let Some(open) = fence {
            push_line(&mut buffer, line);
            if closes_fence(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(heading) = parse_h2(line) {
            if let Some(name) = current.take() {
                insert_section(&mut sections, name, &buffer)?;
                buffer.clear();
            }
            current = Some(heading);
            continue;
        }
        if current.is_none() {
            if line.trim().is_empty() {
                continue;
            }
            if is_h1(line) && !saw_title {
                saw_title = true;
                continue;
            }
            return Err(AgentError::new(
                "invalid_memory_body",
                "记忆分层前只能包含一个 H1 标题。",
            ));
        }
        if is_h1(line) {
            return Err(AgentError::new(
                "invalid_memory_body",
                "H1 标题只能出现在记忆分层之前。",
            ));
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
        }
        push_line(&mut buffer, line);
    }

    if let Some(name) = current {
        insert_section(&mut sections, name, &buffer)?;
    }

    if sections.is_empty() && !body.trim().is_empty() {
        return Err(AgentError::new(
            "invalid_memory_body",
            "记忆正文必须使用约定的 ## 分层 Markdown。",
        ));
    }
    Ok(sections)
}

fn has_recognized_layer_heading(layers: &[&str], body: &str) -> bool {
    let mut fence: Option<Fence> = None;
    for line in body.lines() {
        if let Some(open) = fence {
            if closes_fence(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
            continue;
        }
        if parse_h2(line).is_some_and(|heading| layers.contains(&heading.as_str())) {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy)]
struct Fence {
    marker: char,
    len: usize,
}

fn opens_fence(line: &str) -> Option<Fence> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|char| *char == marker).count();
    (len >= 3).then_some(Fence { marker, len })
}

fn closes_fence(line: &str, fence: Fence) -> bool {
    line.trim_start()
        .chars()
        .take_while(|char| *char == fence.marker)
        .count()
        >= fence.len
}

fn is_h1(line: &str) -> bool {
    line.trim()
        .strip_prefix("# ")
        .is_some_and(|title| !title.trim().is_empty())
}

fn insert_section(
    sections: &mut BTreeMap<String, String>,
    name: String,
    content: &str,
) -> Result<(), AgentError> {
    if sections.contains_key(&name) {
        return Err(AgentError::new(
            "invalid_memory_body",
            format!("记忆 Markdown 含有重复分层：{name}"),
        ));
    }
    sections.insert(name, trim_section(content));
    Ok(())
}

fn push_line(buffer: &mut String, line: &str) {
    if !buffer.is_empty() {
        buffer.push('\n');
    }
    buffer.push_str(line);
}

fn demote_legacy_headings(body: &str) -> String {
    let mut output = Vec::new();
    let mut fence: Option<Fence> = None;
    for line in body.lines() {
        if let Some(open) = fence {
            output.push(line.to_string());
            if closes_fence(line, open) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = opens_fence(line) {
            fence = Some(open);
            output.push(line.to_string());
            continue;
        }
        if is_h1(line) || parse_h2(line).is_some() {
            let indent = line.len() - line.trim_start().len();
            let heading = &line[indent..];
            let prefix = if is_h1(line) { "##" } else { "#" };
            output.push(format!("{}{prefix}{heading}", &line[..indent]));
        } else {
            output.push(line.to_string());
        }
    }
    output.join("\n")
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
        if !allowed.contains(&key.as_str()) {
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
    fn normalizes_project_markdown_and_reads_strict_layers() {
        let body = validate_and_normalize(
            "project",
            "眼睛参数",
            "## Overview\n已完成眼睛参数结构。\n## Stage\n事务提交并完成回读验证。\n## Structure\nParamAngleX 已对齐。\n",
        )
        .unwrap();
        assert!(body.contains("# 眼睛参数"));
        assert!(body.contains("## Structure\n"));
        assert!(body.contains("## Decisions\n"));
        let layers = strict_layers("project", &body).unwrap();
        assert_eq!(layers[0].1, "已完成眼睛参数结构。");
        assert_eq!(layers[1].1, "事务提交并完成回读验证。");
        assert_eq!(layers[2].1, "ParamAngleX 已对齐。");
        for forbidden in ["operationId", "ModelUID", "DocumentUID", "RequestId"] {
            assert!(!body.contains(forbidden));
        }
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
        for body in [
            "前置内容\n## Summary\n摘要",
            "## Summary\n第一份\n## Summary\n第二份",
            "## Summary\n摘要\n# 后置标题",
        ] {
            assert_eq!(
                validate_and_normalize("global", "经验", body)
                    .unwrap_err()
                    .code,
                "invalid_memory_body"
            );
        }
    }

    #[test]
    fn keeps_nested_markdown_and_ignores_headings_inside_fences() {
        let body = validate_and_normalize(
            "global",
            "代码经验",
            "# 代码经验\n\n## Summary\n可复用\n\n## Technique\n### 步骤\n```md\n## Caveats\n原样保留\n```\n\n## Caveats\n实际注意事项",
        )
        .unwrap();
        let technique = select_layers("global", &body, Some(&["Technique".into()])).unwrap();
        assert!(technique.contains("### 步骤"));
        assert!(technique.contains("## Caveats\n原样保留"));
        assert!(body.contains("## Caveats\n实际注意事项"));
    }

    #[test]
    fn wraps_legacy_content_without_turning_old_h2_into_layers() {
        let body = normalize_legacy_body(
            "project",
            "旧阶段",
            "# 旧版标题\n旧版说明\n\n## 自定义标题\n内容\n\n```md\n## 围栏标题\n```",
        )
        .unwrap();
        assert!(body.contains("## Overview\n### 旧版标题\n旧版说明\n\n### 自定义标题\n内容"));
        assert!(body.contains("```md\n## 围栏标题\n```"));
        assert!(body.contains("## Stage\n"));
        validate_and_normalize("project", "旧阶段", &body).unwrap();
    }

    #[test]
    fn migration_rejects_malformed_layered_content_instead_of_wrapping_it() {
        let error = normalize_for_migration(
            "global",
            "损坏记忆",
            "## Summary\n第一份\n## Summary\n第二份",
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_memory_body");

        let legacy = normalize_for_migration(
            "global",
            "旧记忆",
            "旧正文\n\n## 自定义标题\n保留内容",
        )
        .unwrap();
        assert!(legacy.contains("## Summary\n旧正文\n\n### 自定义标题\n保留内容"));
    }

    #[test]
    fn patches_single_layer_without_replacing_other_layers() {
        let body = patch_layer("global", "命名", None, "Summary", "先核对参数 ID。").unwrap();
        let patched = patch_layer(
            "global",
            "命名",
            Some(&body),
            "Technique",
            "列出已有 ID 再新建。",
        )
        .unwrap();
        let layers = strict_layers("global", &patched).unwrap();
        assert_eq!(layers[0].1, "先核对参数 ID。");
        assert_eq!(layers[1].1, "列出已有 ID 再新建。");
        assert!(layers[2].1.is_empty());
    }

    #[test]
    fn builds_ordered_display_layers_and_preserves_legacy_content() {
        let project = layers_for_display(
            "project",
            "## Overview\n项目摘要\n\n## Stage\n已完成参数整理",
        )
        .unwrap();
        assert_eq!(
            project
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            PROJECT_LAYERS
        );
        assert_eq!(project[0].1, "项目摘要");
        assert_eq!(project[1].1, "已完成参数整理");
        assert!(project[2].1.is_empty());

        let legacy = layers_for_display("global", "旧版纯文本经验").unwrap();
        assert_eq!(legacy[0].1, "旧版纯文本经验");
        assert!(legacy[1..].iter().all(|(_, content)| content.is_empty()));

        let unknown =
            layers_for_display("global", "## Summary\n摘要\n\n## Extra\n不可丢失的旧内容").unwrap();
        assert_eq!(
            unknown[0].1,
            "## Summary\n摘要\n\n## Extra\n不可丢失的旧内容"
        );
    }
}
