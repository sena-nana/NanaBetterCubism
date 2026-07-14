use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const EDIT_API_VERSION: &str = "1.1.0";
pub const MAX_BATCH_SIZE: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditorConnectionState {
    Disconnected,
    Connecting,
    AwaitingAccess,
    AwaitingEditPermission,
    Ready,
    Editing,
    Cancelling,
    Incompatible,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorCapabilities {
    pub batch_create_parameters: bool,
    pub find_part_parameters: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ParameterGroupSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorSnapshot {
    pub state: EditorConnectionState,
    pub port: u16,
    pub api_version: Option<String>,
    pub model_label: Option<String>,
    pub groups: Vec<ParameterGroupSummary>,
    pub capabilities: EditorCapabilities,
    pub message: String,
}

impl Default for EditorSnapshot {
    fn default() -> Self {
        Self {
            state: EditorConnectionState::Disconnected,
            port: 22033,
            api_version: None,
            model_label: None,
            groups: Vec::new(),
            capabilities: EditorCapabilities {
                batch_create_parameters: false,
                find_part_parameters: false,
            },
            message: "尚未连接 Cubism Editor。".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdTemplateConfig {
    pub template: String,
    pub prefix: String,
    pub suffix: String,
    pub start_index: u32,
    pub index_width: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchGroupSelection {
    Root,
    Existing { id: String },
    New { id: String, name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowGroupSelection {
    Root,
    Existing { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParameterDefaults {
    pub min: f64,
    pub default: f64,
    pub max: f64,
    pub is_blend_shape: bool,
    pub is_repeat: bool,
    pub group: BatchGroupSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParameterRowOverrides {
    pub min: Option<f64>,
    pub default: Option<f64>,
    pub max: Option<f64>,
    pub is_blend_shape: Option<bool>,
    pub is_repeat: Option<bool>,
    pub group: Option<RowGroupSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParameterInputRow {
    pub client_id: String,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub overrides: ParameterRowOverrides,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParameterBatchInput {
    pub id_template: IdTemplateConfig,
    pub defaults: ParameterDefaults,
    pub rows: Vec<ParameterInputRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub row_id: Option<String>,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParameterPreviewRow {
    pub client_id: String,
    pub name: String,
    pub id: String,
    pub group_id: Option<String>,
    pub group_label: String,
    pub min: f64,
    pub default: f64,
    pub max: f64,
    pub is_blend_shape: bool,
    pub is_repeat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParameterBatchPreview {
    pub preview_id: Option<String>,
    pub model_label: String,
    pub rows: Vec<ParameterPreviewRow>,
    pub new_group: Option<ParameterGroupSummary>,
    pub errors: Vec<ValidationIssue>,
    pub can_execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationAccepted {
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchPhase {
    Validating,
    Beginning,
    CreatingGroup,
    CreatingParameters,
    Committing,
    Verifying,
    Cancelling,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgress {
    pub operation_id: String,
    pub phase: BatchPhase,
    pub completed: usize,
    pub total: usize,
    pub current_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    Committed,
    CancelledRolledBack,
    FailedRolledBack,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatchFinished {
    pub operation_id: String,
    pub outcome: BatchOutcome,
    pub created_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExistingParameter {
    pub id: String,
    pub name: String,
    pub group_id: Option<String>,
    pub min: f64,
    pub default: f64,
    pub max: f64,
    pub is_blend_shape: bool,
    pub is_repeat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelStructure {
    pub groups: Vec<ParameterGroupSummary>,
    pub parameters: Vec<ExistingParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct PartSelectionSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartAssociatedObject {
    pub id: String,
    pub name: String,
    pub object_type: String,
    pub key_values: Vec<f64>,
    pub source_part_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartAssociatedParameter {
    pub id: String,
    pub name: String,
    pub group: Option<ParameterGroupSummary>,
    pub key_values: Vec<f64>,
    pub objects: Vec<PartAssociatedObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PartParameterQueryResult {
    pub model_label: String,
    pub selected_parts: Vec<PartSelectionSummary>,
    pub ignored_selection_count: usize,
    pub scanned_object_count: usize,
    pub parameters: Vec<PartAssociatedParameter>,
}

impl ModelStructure {
    pub fn semantic_hash(&self) -> String {
        let mut normalized = self.clone();
        normalized.groups.sort();
        normalized
            .parameters
            .sort_by(|left, right| left.id.cmp(&right.id));
        let bytes = serde_json::to_vec(&normalized).unwrap_or_default();
        format!("{:x}", Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone)]
pub struct StoredPlan {
    pub preview_id: String,
    pub generation: u64,
    pub model_uid: String,
    pub structure_hash: String,
    pub new_group: Option<ParameterGroupSummary>,
    pub rows: Vec<ParameterPreviewRow>,
}

pub fn build_preview(
    input: &ParameterBatchInput,
    structure: &ModelStructure,
    model_label: &str,
) -> ParameterBatchPreview {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let existing_parameter_ids: BTreeSet<_> = structure
        .parameters
        .iter()
        .map(|parameter| parameter.id.as_str())
        .collect();
    let groups: BTreeMap<_, _> = structure
        .groups
        .iter()
        .map(|group| (group.id.as_str(), group.name.as_str()))
        .collect();

    if input.rows.is_empty() {
        errors.push(issue("empty_batch", "至少需要一个参数。", None, None));
    }
    if input.rows.len() > MAX_BATCH_SIZE {
        errors.push(issue(
            "batch_too_large",
            format!("单批最多创建 {MAX_BATCH_SIZE} 个参数。"),
            None,
            None,
        ));
    }
    if !(1..=6).contains(&input.id_template.index_width) {
        errors.push(issue(
            "invalid_index_width",
            "编号补零位数必须在 1 到 6 之间。",
            None,
            Some("idTemplate.indexWidth"),
        ));
    }
    if let Err(message) = validate_template(&input.id_template.template) {
        errors.push(issue(
            "invalid_template",
            message,
            None,
            Some("idTemplate.template"),
        ));
    }

    let new_group = match &input.defaults.group {
        BatchGroupSelection::New { id, name } => {
            if name.trim().is_empty() {
                errors.push(issue(
                    "empty_group_name",
                    "新参数组名称不能为空。",
                    None,
                    Some("defaults.group.name"),
                ));
            }
            if let Err(message) = validate_identifier(id) {
                errors.push(issue(
                    "invalid_group_id",
                    message,
                    None,
                    Some("defaults.group.id"),
                ));
            }
            if groups.contains_key(id.as_str()) || existing_parameter_ids.contains(id.as_str()) {
                errors.push(issue(
                    "group_id_conflict",
                    format!("ID {id} 已存在于当前模型。"),
                    None,
                    Some("defaults.group.id"),
                ));
            }
            Some(ParameterGroupSummary {
                id: id.clone(),
                name: name.trim().to_string(),
            })
        }
        BatchGroupSelection::Existing { id } if !groups.contains_key(id.as_str()) => {
            errors.push(issue(
                "missing_group",
                format!("参数组 {id} 不存在或模型结构已变化。"),
                None,
                Some("defaults.group"),
            ));
            None
        }
        _ => None,
    };

    let mut generated_ids = BTreeSet::new();
    for (position, row) in input.rows.iter().take(MAX_BATCH_SIZE).enumerate() {
        let row_id = if row.client_id.is_empty() {
            format!("row-{position}")
        } else {
            row.client_id.clone()
        };
        let name = row.name.trim();
        if name.is_empty() {
            errors.push(issue(
                "empty_name",
                "参数名称不能为空。",
                Some(row_id.clone()),
                Some("name"),
            ));
        }

        let index = input.id_template.start_index.checked_add(position as u32);
        let generated_id =
            match index.and_then(|index| expand_template(&input.id_template, row, index).ok()) {
                Some(id) => id,
                None => {
                    errors.push(issue(
                        "index_overflow",
                        "编号超出支持范围。",
                        Some(row_id.clone()),
                        Some("id"),
                    ));
                    String::new()
                }
            };

        if let Err(message) = validate_identifier(&generated_id) {
            errors.push(issue(
                "invalid_parameter_id",
                message,
                Some(row_id.clone()),
                Some("id"),
            ));
        } else {
            if new_group
                .as_ref()
                .is_some_and(|group| group.id == generated_id)
            {
                errors.push(issue(
                    "parameter_group_id_conflict",
                    format!("参数 ID {generated_id} 与本批次新参数组 ID 冲突。"),
                    Some(row_id.clone()),
                    Some("id"),
                ));
            }
            if existing_parameter_ids.contains(generated_id.as_str())
                || groups.contains_key(generated_id.as_str())
            {
                errors.push(issue(
                    "parameter_id_conflict",
                    format!("ID {generated_id} 已存在于当前模型。"),
                    Some(row_id.clone()),
                    Some("id"),
                ));
            }
            if !generated_ids.insert(generated_id.clone()) {
                errors.push(issue(
                    "duplicate_parameter_id",
                    format!("本批次重复生成了 ID {generated_id}。"),
                    Some(row_id.clone()),
                    Some("id"),
                ));
            }
        }

        let min = row.overrides.min.unwrap_or(input.defaults.min);
        let default = row.overrides.default.unwrap_or(input.defaults.default);
        let max = row.overrides.max.unwrap_or(input.defaults.max);
        if !min.is_finite()
            || !default.is_finite()
            || !max.is_finite()
            || min > default
            || default > max
        {
            errors.push(issue(
                "invalid_range",
                "参数范围必须是有限数值，并满足最小值 ≤ 默认值 ≤ 最大值。",
                Some(row_id.clone()),
                Some("range"),
            ));
        }

        let (group_id, group_label) = resolve_group(
            row.overrides.group.as_ref(),
            &input.defaults.group,
            &groups,
            &new_group,
            &row_id,
            &mut errors,
        );

        rows.push(ParameterPreviewRow {
            client_id: row_id,
            name: name.to_string(),
            id: generated_id,
            group_id,
            group_label,
            min,
            default,
            max,
            is_blend_shape: row
                .overrides
                .is_blend_shape
                .unwrap_or(input.defaults.is_blend_shape),
            is_repeat: row.overrides.is_repeat.unwrap_or(input.defaults.is_repeat),
        });
    }

    ParameterBatchPreview {
        preview_id: None,
        model_label: model_label.to_string(),
        rows,
        new_group,
        can_execute: errors.is_empty(),
        errors,
    }
}

fn resolve_group(
    override_group: Option<&RowGroupSelection>,
    default_group: &BatchGroupSelection,
    groups: &BTreeMap<&str, &str>,
    new_group: &Option<ParameterGroupSummary>,
    row_id: &str,
    errors: &mut Vec<ValidationIssue>,
) -> (Option<String>, String) {
    match override_group {
        Some(RowGroupSelection::Root) => (None, "根级".into()),
        Some(RowGroupSelection::Existing { id }) => match groups.get(id.as_str()) {
            Some(name) => (Some(id.clone()), (*name).to_string()),
            None => {
                errors.push(issue(
                    "missing_row_group",
                    format!("参数组 {id} 不存在或模型结构已变化。"),
                    Some(row_id.to_string()),
                    Some("group"),
                ));
                (Some(id.clone()), id.clone())
            }
        },
        None => match default_group {
            BatchGroupSelection::Root => (None, "根级".into()),
            BatchGroupSelection::Existing { id } => (
                Some(id.clone()),
                groups.get(id.as_str()).copied().unwrap_or(id).to_string(),
            ),
            BatchGroupSelection::New { id, .. } => (
                Some(id.clone()),
                new_group
                    .as_ref()
                    .map(|group| group.name.clone())
                    .unwrap_or_else(|| id.clone()),
            ),
        },
    }
}

pub fn validate_template(template: &str) -> Result<(), String> {
    if template.is_empty() {
        return Err("ID 模板不能为空。".into());
    }
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        if remaining[..open].contains('}') {
            return Err("ID 模板包含多余的右花括号。".into());
        }
        let after_open = &remaining[open + 1..];
        let Some(close) = after_open.find('}') else {
            return Err("ID 模板包含未闭合的令牌。".into());
        };
        let token = &after_open[..close];
        if !matches!(token, "prefix" | "key" | "side" | "index" | "suffix") {
            return Err(format!("不支持模板令牌 {{{token}}}。"));
        }
        remaining = &after_open[close + 1..];
    }
    if remaining.contains('}') {
        return Err("ID 模板包含多余的右花括号。".into());
    }
    Ok(())
}

pub fn expand_template(
    config: &IdTemplateConfig,
    row: &ParameterInputRow,
    index: u32,
) -> Result<String, String> {
    validate_template(&config.template)?;
    let index = format!("{:0width$}", index, width = config.index_width as usize);
    Ok(config
        .template
        .replace("{prefix}", &config.prefix)
        .replace("{key}", row.key.trim())
        .replace("{side}", row.side.trim())
        .replace("{index}", &index)
        .replace("{suffix}", &config.suffix))
}

pub fn validate_identifier(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 63 {
        return Err("ID 长度必须在 1 到 63 个单字节字符之间。".into());
    }
    if id.as_bytes()[0].is_ascii_digit() {
        return Err("ID 不能以数字开头。".into());
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("ID 只能包含单字节字母、数字和下划线。".into());
    }
    Ok(())
}

fn issue(
    code: impl Into<String>,
    message: impl Into<String>,
    row_id: Option<String>,
    field: Option<&str>,
) -> ValidationIssue {
    ValidationIssue {
        code: code.into(),
        message: message.into(),
        row_id,
        field: field.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ParameterBatchInput {
        ParameterBatchInput {
            id_template: IdTemplateConfig {
                template: "{prefix}{key}{side}{index}{suffix}".into(),
                prefix: "Param".into(),
                suffix: String::new(),
                start_index: 1,
                index_width: 2,
            },
            defaults: ParameterDefaults {
                min: -1.0,
                default: 0.0,
                max: 1.0,
                is_blend_shape: false,
                is_repeat: false,
                group: BatchGroupSelection::Root,
            },
            rows: vec![ParameterInputRow {
                client_id: "row-a".into(),
                name: "前发摆动".into(),
                key: "Hair".into(),
                side: "L".into(),
                overrides: ParameterRowOverrides::default(),
            }],
        }
    }

    #[test]
    fn expands_supported_tokens_and_padding() {
        let preview = build_preview(&input(), &ModelStructure::default(), "当前模型");
        assert!(preview.errors.is_empty());
        assert_eq!(preview.rows[0].id, "ParamHairL01");
    }

    #[test]
    fn rejects_existing_and_batch_duplicate_ids() {
        let mut value = input();
        value.rows.push(value.rows[0].clone());
        value.id_template.template = "{prefix}{key}{side}".into();
        let structure = ModelStructure {
            parameters: vec![ExistingParameter {
                id: "ParamHairL".into(),
                name: "Existing".into(),
                group_id: None,
                min: -1.0,
                default: 0.0,
                max: 1.0,
                is_blend_shape: false,
                is_repeat: false,
            }],
            ..Default::default()
        };
        let preview = build_preview(&value, &structure, "当前模型");
        assert!(!preview.can_execute);
        assert!(preview
            .errors
            .iter()
            .any(|error| error.code == "parameter_id_conflict"));
        assert!(preview
            .errors
            .iter()
            .any(|error| error.code == "duplicate_parameter_id"));
    }

    #[test]
    fn rejects_invalid_template_range_and_group() {
        let mut value = input();
        value.id_template.template = "{unknown}".into();
        value.defaults.min = 2.0;
        value.defaults.max = 1.0;
        value.defaults.group = BatchGroupSelection::Existing {
            id: "Missing".into(),
        };
        let preview = build_preview(&value, &ModelStructure::default(), "当前模型");
        let codes: BTreeSet<_> = preview
            .errors
            .iter()
            .map(|error| error.code.as_str())
            .collect();
        assert!(codes.contains("invalid_template"));
        assert!(codes.contains("invalid_range"));
        assert!(codes.contains("missing_group"));
    }

    #[test]
    fn enforces_cubism_identifier_rules() {
        assert!(validate_identifier("ParamAngleX").is_ok());
        assert!(validate_identifier("_Param_01").is_ok());
        assert!(validate_identifier("1Param").is_err());
        assert!(validate_identifier("参数").is_err());
        assert!(validate_identifier(&"A".repeat(64)).is_err());
        assert!(validate_template("Param}{key}").is_err());
    }

    #[test]
    fn enforces_batch_limit_and_new_group_namespace() {
        let mut value = input();
        value.rows = (0..=MAX_BATCH_SIZE)
            .map(|position| ParameterInputRow {
                client_id: format!("row-{position}"),
                name: format!("参数 {position}"),
                key: "Angle".into(),
                side: String::new(),
                overrides: ParameterRowOverrides::default(),
            })
            .collect();
        value.defaults.group = BatchGroupSelection::New {
            id: "ParamAngle01".into(),
            name: "角度".into(),
        };
        let preview = build_preview(&value, &ModelStructure::default(), "当前模型");

        assert!(preview
            .errors
            .iter()
            .any(|error| error.code == "batch_too_large"));
        assert!(preview
            .errors
            .iter()
            .any(|error| error.code == "parameter_group_id_conflict"));
    }

    #[test]
    fn serializes_only_domain_snapshot_fields() {
        let value = serde_json::to_value(EditorSnapshot::default()).unwrap();
        assert_eq!(value["state"], "disconnected");
        assert_eq!(value["capabilities"]["batchCreateParameters"], false);
        assert_eq!(value["capabilities"]["findPartParameters"], false);
        assert!(value.get("modelUid").is_none());
        assert!(value.get("documentUid").is_none());
        assert!(value.get("token").is_none());
    }
}
