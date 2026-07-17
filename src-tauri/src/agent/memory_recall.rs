use crate::agent::memory_markdown;
use crate::agent::store::MemoryRecord;
use crate::agent::AgentError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const DEFAULT_RECALL_LIMIT: usize = 5;
pub const MAX_RECALL_LIMIT: usize = 8;
pub const RECALL_CONTENT_BUDGET: usize = 12_000;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallDepth {
    Index,
    #[default]
    Focused,
    Full,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRecallScope {
    #[default]
    All,
    Project,
    Global,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRecallRequest {
    pub query: String,
    #[serde(default)]
    pub depth: MemoryRecallDepth,
    #[serde(default)]
    pub scope: MemoryRecallScope,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallLayer {
    pub name: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallMatch {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub title: String,
    pub layers: Vec<MemoryRecallLayer>,
    pub updated_at: String,
    pub revision: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallResult {
    pub query: String,
    pub depth: MemoryRecallDepth,
    pub scope: MemoryRecallScope,
    pub matches: Vec<MemoryRecallMatch>,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct MemoryRecallSource {
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub updated_at: String,
    pub revision: i64,
}

struct QueryFeatures {
    compact: String,
    tokens: BTreeSet<String>,
    cjk_grams: BTreeSet<String>,
}

struct RankedMemory {
    memory: MemoryRecallSource,
    layers: Vec<(String, String)>,
    layer_scores: Vec<u64>,
    score: u64,
}

pub fn recall_memories(
    memories: Vec<MemoryRecallSource>,
    request: MemoryRecallRequest,
) -> Result<MemoryRecallResult, AgentError> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err(AgentError::new(
            "invalid_arguments",
            "recall_memory 的 query 不能为空。",
        ));
    }
    let limit = request.limit.unwrap_or(DEFAULT_RECALL_LIMIT);
    if !(1..=MAX_RECALL_LIMIT).contains(&limit) {
        return Err(AgentError::new(
            "invalid_arguments",
            format!("recall_memory 的 limit 必须在 1 到 {MAX_RECALL_LIMIT} 之间。"),
        ));
    }

    let features = QueryFeatures::new(&query);
    if features.compact.is_empty() {
        return Err(AgentError::new(
            "invalid_arguments",
            "recall_memory 的 query 必须包含文字或数字。",
        ));
    }
    let mut ranked = Vec::new();
    for memory in memories
        .into_iter()
        .filter(|memory| request.scope.includes(&memory.scope))
    {
        let layers = memory_markdown::strict_layers(&memory.scope, &memory.body)?;
        let title_score = features.score(&memory.title);
        let layer_scores = layers
            .iter()
            .map(|(_, content)| features.score(content))
            .collect::<Vec<_>>();
        let index_score = layer_scores.first().copied().unwrap_or_default();
        let deep_score = layer_scores.iter().skip(1).sum::<u64>();
        let score = title_score * 5 + index_score * 3 + deep_score * 2;
        if score > 0 {
            ranked.push(RankedMemory {
                memory,
                layers,
                layer_scores,
                score,
            });
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| scope_order(&left.memory.scope).cmp(&scope_order(&right.memory.scope)))
            .then_with(|| right.memory.updated_at.cmp(&left.memory.updated_at))
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });

    let mut remaining = RECALL_CONTENT_BUDGET;
    let mut result_truncated = ranked.len() > limit;
    let mut matches = Vec::new();
    for ranked_memory in ranked.into_iter().take(limit) {
        if remaining == 0 {
            result_truncated = true;
            break;
        }
        let selected = selected_layer_indexes(
            request.depth,
            &ranked_memory.layers,
            &ranked_memory.layer_scores,
        );
        let mut layers = Vec::new();
        let mut match_truncated = false;
        for index in selected {
            let (name, content) = &ranked_memory.layers[index];
            if content.is_empty() {
                continue;
            }
            if remaining == 0 {
                match_truncated = true;
                result_truncated = true;
                break;
            }
            let (content, truncated) = take_chars(content, remaining);
            remaining = remaining.saturating_sub(content.chars().count());
            layers.push(MemoryRecallLayer {
                name: name.clone(),
                content,
                truncated,
            });
            if truncated {
                match_truncated = true;
                result_truncated = true;
                break;
            }
        }
        let memory = ranked_memory.memory;
        matches.push(MemoryRecallMatch {
            id: memory.id,
            scope: memory.scope,
            kind: memory.kind,
            title: memory.title,
            layers,
            updated_at: memory.updated_at,
            revision: memory.revision,
            truncated: match_truncated,
        });
    }

    Ok(MemoryRecallResult {
        query,
        depth: request.depth,
        scope: request.scope,
        matches,
        truncated: result_truncated,
    })
}

impl From<MemoryRecord> for MemoryRecallSource {
    fn from(memory: MemoryRecord) -> Self {
        Self {
            id: memory.id,
            scope: memory.scope,
            kind: memory.kind,
            title: memory.title,
            body: memory.body,
            updated_at: memory.updated_at,
            revision: memory.revision,
        }
    }
}

impl MemoryRecallScope {
    fn includes(self, scope: &str) -> bool {
        matches!(
            (self, scope),
            (Self::All, "project" | "global")
                | (Self::Project, "project")
                | (Self::Global, "global")
        )
    }
}

impl QueryFeatures {
    fn new(query: &str) -> Self {
        let compact = compact_text(query);
        let tokens = query_tokens(query);
        let cjk_grams = cjk_grams(query);
        Self {
            compact,
            tokens,
            cjk_grams,
        }
    }

    fn score(&self, text: &str) -> u64 {
        let compact = compact_text(text);
        if compact.is_empty() {
            return 0;
        }
        let mut score = 0;
        if compact.contains(&self.compact) {
            score += 100;
        }
        for token in &self.tokens {
            if token.chars().count() >= 2 && compact.contains(token) {
                score += 20;
            }
        }
        for gram in &self.cjk_grams {
            if compact.contains(gram) {
                score += 4;
            }
        }
        score
    }
}

fn selected_layer_indexes(
    depth: MemoryRecallDepth,
    layers: &[(String, String)],
    scores: &[u64],
) -> Vec<usize> {
    match depth {
        MemoryRecallDepth::Index => vec![0],
        MemoryRecallDepth::Focused => std::iter::once(0)
            .chain((1..layers.len()).filter(|index| scores[*index] > 0))
            .collect(),
        MemoryRecallDepth::Full => (0..layers.len()).collect(),
    }
}

fn take_chars(value: &str, limit: usize) -> (String, bool) {
    let mut chars = value.chars();
    let content = chars.by_ref().take(limit).collect::<String>();
    (content, chars.next().is_some())
}

fn compact_text(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn query_tokens(value: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() && !is_cjk(character) {
            current.push(character);
        } else if !current.is_empty() {
            tokens.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens
}

fn cjk_grams(value: &str) -> BTreeSet<String> {
    let mut grams = BTreeSet::new();
    let mut run = Vec::new();
    let flush = |run: &mut Vec<char>, grams: &mut BTreeSet<String>| {
        if run.len() == 1 {
            grams.insert(run.iter().collect());
        } else {
            for pair in run.windows(2) {
                grams.insert(pair.iter().collect());
            }
        }
        run.clear();
    };
    for character in value.chars() {
        if is_cjk(character) {
            run.push(character);
        } else if !run.is_empty() {
            flush(&mut run, &mut grams);
        }
    }
    if !run.is_empty() {
        flush(&mut run, &mut grams);
    }
    grams
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff
    )
}

fn scope_order(scope: &str) -> u8 {
    if scope == "project" {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(
        id: &str,
        scope: &str,
        title: &str,
        body: &str,
        updated_at: &str,
    ) -> MemoryRecallSource {
        MemoryRecallSource {
            id: id.into(),
            scope: scope.into(),
            kind: if scope == "project" {
                "stage"
            } else {
                "experience"
            }
            .into(),
            title: title.into(),
            body: body.into(),
            updated_at: updated_at.into(),
            revision: 1,
        }
    }

    fn request(query: &str, depth: MemoryRecallDepth) -> MemoryRecallRequest {
        MemoryRecallRequest {
            query: query.into(),
            depth,
            scope: MemoryRecallScope::All,
            limit: None,
        }
    }

    #[test]
    fn focused_recall_matches_cjk_and_identifiers_without_unrelated_layers() {
        let result = recall_memories(
            vec![
                memory(
                    "eyes",
                    "project",
                    "眼睛参数",
                    "## Overview\n眼睛参数结构已建立。\n## Stage\nParamAngleX 已对齐。\n## Structure\n参数位于 Face 组。\n## Decisions\n采用默认范围。",
                    "2026-07-16T10:00:00Z",
                ),
                memory(
                    "mouth",
                    "project",
                    "嘴部参数",
                    "## Overview\n嘴部开合已完成。\n## Stage\nParamMouthOpenY 已对齐。",
                    "2026-07-16T11:00:00Z",
                ),
            ],
            request("眼睛 ParamAngleX", MemoryRecallDepth::Focused),
        )
        .unwrap();

        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].id, "eyes");
        assert_eq!(
            result.matches[0]
                .layers
                .iter()
                .map(|layer| layer.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Overview", "Stage"]
        );
    }

    #[test]
    fn depth_and_scope_control_returned_memory_layers() {
        let source = memory(
            "naming",
            "global",
            "参数命名",
            "## Summary\n先核对参数 ID。\n## Technique\n列出已有 ID 再新建。\n## Caveats\n避免重名。",
            "2026-07-16T10:00:00Z",
        );
        let index = recall_memories(
            vec![source.clone()],
            MemoryRecallRequest {
                scope: MemoryRecallScope::Global,
                ..request("参数 ID", MemoryRecallDepth::Index)
            },
        )
        .unwrap();
        assert_eq!(index.matches[0].layers.len(), 1);

        let full = recall_memories(
            vec![source],
            MemoryRecallRequest {
                scope: MemoryRecallScope::Global,
                ..request("参数 ID", MemoryRecallDepth::Full)
            },
        )
        .unwrap();
        assert_eq!(full.matches[0].layers.len(), 3);
        assert!(recall_memories(
            vec![memory(
                "project",
                "project",
                "参数命名",
                "## Overview\n项目参数命名。",
                "2026-07-16T10:00:00Z",
            )],
            MemoryRecallRequest {
                scope: MemoryRecallScope::Global,
                ..request("参数命名", MemoryRecallDepth::Focused)
            },
        )
        .unwrap()
        .matches
        .is_empty());
    }

    #[test]
    fn recall_enforces_limit_budget_and_strict_layered_markdown() {
        let oversized = "参数".repeat(RECALL_CONTENT_BUDGET);
        let result = recall_memories(
            vec![
                memory(
                    "a",
                    "global",
                    "参数经验 A",
                    &format!("## Summary\n{oversized}\n## Technique\n参数技巧"),
                    "2026-07-16T10:00:00Z",
                ),
                memory(
                    "b",
                    "global",
                    "参数经验 B",
                    "## Summary\n参数摘要",
                    "2026-07-16T09:00:00Z",
                ),
            ],
            MemoryRecallRequest {
                limit: Some(1),
                ..request("参数", MemoryRecallDepth::Full)
            },
        )
        .unwrap();
        assert!(result.truncated);
        assert!(result.matches[0].truncated);
        assert!(result.matches[0].layers[0].truncated);
        assert_eq!(
            result.matches[0].layers[0].content.chars().count(),
            RECALL_CONTENT_BUDGET
        );

        assert!(matches!(
            recall_memories(
                vec![memory(
                    "legacy",
                    "global",
                    "旧记忆",
                    "纯文本参数经验",
                    "2026-07-16T10:00:00Z",
                )],
                request("参数", MemoryRecallDepth::Focused),
            ),
            Err(error) if error.code == "invalid_memory_body"
        ));
        assert!(matches!(
            recall_memories(Vec::new(), request("...", MemoryRecallDepth::Focused)),
            Err(error) if error.code == "invalid_arguments"
        ));
    }

    #[test]
    fn recall_uses_default_and_maximum_limits() {
        let memories = (0..9)
            .map(|index| {
                memory(
                    &format!("memory-{index}"),
                    "global",
                    &format!("参数经验 {index}"),
                    "## Summary\n参数摘要",
                    &format!("2026-07-16T{index:02}:00:00Z"),
                )
            })
            .collect::<Vec<_>>();

        let default = recall_memories(
            memories.clone(),
            request("参数", MemoryRecallDepth::Index),
        )
        .unwrap();
        assert_eq!(default.matches.len(), DEFAULT_RECALL_LIMIT);
        assert!(default.truncated);

        let maximum = recall_memories(
            memories.clone(),
            MemoryRecallRequest {
                limit: Some(MAX_RECALL_LIMIT),
                ..request("参数", MemoryRecallDepth::Index)
            },
        )
        .unwrap();
        assert_eq!(maximum.matches.len(), MAX_RECALL_LIMIT);
        assert!(maximum.truncated);

        assert!(matches!(
            recall_memories(
                memories,
                MemoryRecallRequest {
                    limit: Some(MAX_RECALL_LIMIT + 1),
                    ..request("参数", MemoryRecallDepth::Index)
                },
            ),
            Err(error) if error.code == "invalid_arguments"
        ));
    }

    #[test]
    fn request_defaults_are_focused_and_reject_unknown_fields() {
        let parsed: MemoryRecallRequest = serde_json::from_value(serde_json::json!({
            "query": "眼睛参数"
        }))
        .unwrap();
        assert_eq!(parsed.depth, MemoryRecallDepth::Focused);
        assert_eq!(parsed.scope, MemoryRecallScope::All);
        assert_eq!(parsed.limit, None);
        assert!(
            serde_json::from_value::<MemoryRecallRequest>(serde_json::json!({
                "query": "眼睛参数",
                "layers": ["Stage"]
            }))
            .is_err()
        );
    }
}
