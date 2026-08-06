use super::CommandError;
use crate::domain::{validate_identifier, CUBISM_ID_MAX_LEN, CUBISM_ID_PATTERN, MAX_BATCH_SIZE};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolMode {
    Direct,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolAccess {
    ReadOnly,
    Mutating,
}

#[derive(Clone, Copy)]
enum FieldKind {
    String { max_len: Option<usize> },
    Identifier,
    Number { min: Option<f64>, max: Option<f64> },
    Integer { min: i64, max: i64 },
    Boolean,
    IdentifierArray,
    ParameterValues,
    ParameterFilters,
    Color,
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy)]
pub(super) struct FieldSpec {
    pub(super) input: &'static str,
    pub(super) editor: &'static str,
    kind: FieldKind,
    pub(super) required: bool,
}

pub(super) struct ToolSpec {
    pub(super) tool_name: &'static str,
    pub(super) method: &'static str,
    pub(super) description: &'static str,
    pub(super) display_name: &'static str,
    pub(super) mode: ToolMode,
    pub(super) access: ToolAccess,
    pub(super) fields: Vec<FieldSpec>,
    pub(super) uses_model: bool,
    pub(super) destructive: bool,
}

pub(super) const LABEL_COLORS: &[&str] = &[
    "Undefined",
    "Red",
    "Orange",
    "Yellow",
    "Green",
    "Blue",
    "Purple",
    "Gray",
    "Custom",
];
pub(super) const COLOR_BLENDS: &[&str] = &[
    "Normal",
    "Add",
    "AddGlow",
    "Darken",
    "Multiply",
    "ColorBurn",
    "LinearBurn",
    "Lighten",
    "Screen",
    "ColorDodge",
    "Overlay",
    "SoftLight",
    "HardLight",
    "LinearLight",
    "Hue",
    "Color",
    "Add_5.2",
    "Multiply_5.2",
];
pub(super) const ALPHA_BLENDS: &[&str] = &["Over", "Atop", "Out", "Conjoint", "Disjoint"];
pub(super) const DEFORMER_MODES: &[&str] = &["AsParent", "AsChild"];
pub(super) const LOG_TYPES: &[&str] = &["info", "warning"];

impl FieldSpec {
    fn new(input: &'static str, editor: &'static str, required: bool, kind: FieldKind) -> Self {
        Self {
            input,
            editor,
            kind,
            required,
        }
    }
}

pub(super) fn string(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::String { max_len: None })
}

pub(super) fn identifier(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Identifier)
}

pub(super) fn limited_string(
    input: &'static str,
    editor: &'static str,
    required: bool,
    max_len: usize,
) -> FieldSpec {
    FieldSpec::new(
        input,
        editor,
        required,
        FieldKind::String {
            max_len: Some(max_len),
        },
    )
}

pub(super) fn number(
    input: &'static str,
    editor: &'static str,
    required: bool,
    min: Option<f64>,
    max: Option<f64>,
) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Number { min, max })
}

pub(super) fn integer(
    input: &'static str,
    editor: &'static str,
    required: bool,
    min: i64,
    max: i64,
) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Integer { min, max })
}

pub(super) fn boolean(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Boolean)
}

pub(super) fn identifiers(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::IdentifierArray)
}

pub(super) fn parameter_values(
    input: &'static str,
    editor: &'static str,
    required: bool,
) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::ParameterValues)
}

pub(super) fn parameter_filters(
    input: &'static str,
    editor: &'static str,
    required: bool,
) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::ParameterFilters)
}

pub(super) fn color(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Color)
}

pub(super) fn choice(
    input: &'static str,
    editor: &'static str,
    required: bool,
    choices: &'static [&'static str],
) -> FieldSpec {
    FieldSpec::new(input, editor, required, FieldKind::Choice(choices))
}

pub(super) fn query(
    tool_name: &'static str,
    display_name: &'static str,
    method: &'static str,
    description: &'static str,
    uses_model: bool,
    fields: Vec<FieldSpec>,
) -> ToolSpec {
    ToolSpec {
        tool_name,
        method,
        description,
        display_name,
        mode: ToolMode::Direct,
        access: ToolAccess::ReadOnly,
        fields,
        uses_model,
        destructive: false,
    }
}

pub(super) fn effect(
    tool_name: &'static str,
    display_name: &'static str,
    method: &'static str,
    description: &'static str,
    uses_model: bool,
    fields: Vec<FieldSpec>,
) -> ToolSpec {
    ToolSpec {
        tool_name,
        method,
        description,
        display_name,
        mode: ToolMode::Direct,
        access: ToolAccess::Mutating,
        fields,
        uses_model,
        destructive: false,
    }
}

pub(super) fn preview(
    tool_name: &'static str,
    display_name: &'static str,
    method: &'static str,
    description: &'static str,
    destructive: bool,
    fields: Vec<FieldSpec>,
) -> ToolSpec {
    ToolSpec {
        tool_name,
        method,
        description,
        display_name,
        mode: ToolMode::Preview,
        access: ToolAccess::Mutating,
        fields,
        uses_model: true,
        destructive,
    }
}

pub(super) fn common_object_edit_fields(is_exact_match_key: &'static str) -> Vec<FieldSpec> {
    vec![
        identifier("id", "Id", true),
        parameter_filters("parameters", "Parameters", false),
        boolean("isExactMatch", is_exact_match_key, false),
        identifier("newId", "NewId", false),
        string("name", "Name", false),
        identifier("parentId", "ParentId", false),
    ]
}

fn identifier_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": CUBISM_ID_MAX_LEN,
        "pattern": CUBISM_ID_PATTERN
    })
}

pub(super) fn field_schema(field: &FieldSpec) -> Value {
    match field.kind {
        FieldKind::String { max_len } => {
            let mut schema = json!({"type": "string", "minLength": 1});
            if let Some(max_len) = max_len {
                schema["maxLength"] = json!(max_len);
            }
            schema
        }
        FieldKind::Identifier => identifier_schema(),
        FieldKind::Number { min, max } => {
            let mut schema = json!({"type": "number"});
            if let Some(min) = min {
                schema["minimum"] = json!(min);
            }
            if let Some(max) = max {
                schema["maximum"] = json!(max);
            }
            schema
        }
        FieldKind::Integer { min, max } => {
            json!({"type": "integer", "minimum": min, "maximum": max})
        }
        FieldKind::Boolean => json!({"type": "boolean"}),
        FieldKind::IdentifierArray => {
            json!({"type": "array", "items": identifier_schema()})
        }
        FieldKind::ParameterValues => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": identifier_schema(),
                    "value": {"type": "number"}
                },
                "required": ["id", "value"],
                "additionalProperties": false
            }
        }),
        FieldKind::ParameterFilters => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": identifier_schema(),
                    "value": {"type": "number"}
                },
                "anyOf": [{"required": ["id"]}, {"required": ["value"]}],
                "additionalProperties": false
            }
        }),
        FieldKind::Color => json!({"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"}),
        FieldKind::Choice(values) => json!({"type": "string", "enum": values}),
    }
}

pub(super) fn function_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}
pub(super) fn validate_value(field: &FieldSpec, value: &Value) -> Result<Value, CommandError> {
    let invalid = || {
        CommandError::new(
            "invalid_arguments",
            format!("参数 {} 的类型或范围不符合官方 API 定义。", field.input),
        )
    };
    match field.kind {
        FieldKind::String { max_len } => {
            let value = value.as_str().ok_or_else(invalid)?;
            if value.is_empty() || max_len.is_some_and(|max| value.chars().count() > max) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Identifier => {
            let value = value.as_str().ok_or_else(invalid)?;
            validate_cubism_identifier(field, value, None)?;
            Ok(json!(value))
        }
        FieldKind::Number { min, max } => {
            let value = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(invalid)?;
            if min.is_some_and(|min| value < min) || max.is_some_and(|max| value > max) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Integer { min, max } => {
            let value = value.as_i64().ok_or_else(invalid)?;
            if value < min || value > max {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Boolean => value.as_bool().map(Value::Bool).ok_or_else(invalid),
        FieldKind::IdentifierArray => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let value = value.as_str().ok_or_else(invalid)?;
                validate_cubism_identifier(field, value, Some(index))?;
                normalized.push(json!(value));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::ParameterValues => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let object = value.as_object().ok_or_else(invalid)?;
                if object.keys().any(|key| key != "id" && key != "value") {
                    return Err(invalid());
                }
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(invalid)?;
                validate_cubism_identifier(field, id, Some(index))?;
                let number = object
                    .get("value")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                    .ok_or_else(invalid)?;
                normalized.push(json!({"Id": id, "Value": number}));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::ParameterFilters => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let object = value.as_object().ok_or_else(invalid)?;
                if object.is_empty() || object.keys().any(|key| key != "id" && key != "value") {
                    return Err(invalid());
                }
                let mut item = Map::new();
                if let Some(id) = object.get("id") {
                    let id = id
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(invalid)?;
                    validate_cubism_identifier(field, id, Some(index))?;
                    item.insert("Id".into(), json!(id));
                }
                if let Some(number) = object.get("value") {
                    let number = number
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(invalid)?;
                    item.insert("Value".into(), json!(number));
                }
                normalized.push(Value::Object(item));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::Color => {
            let value = value.as_str().ok_or_else(invalid)?;
            let bytes = value.as_bytes();
            if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(invalid());
            }
            Ok(json!(value.to_ascii_uppercase()))
        }
        FieldKind::Choice(choices) => {
            let value = value.as_str().ok_or_else(invalid)?;
            if !choices.contains(&value) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
    }
}

fn validate_cubism_identifier(
    field: &FieldSpec,
    value: &str,
    index: Option<usize>,
) -> Result<(), CommandError> {
    validate_identifier(value).map_err(|message| {
        let location = index.map_or_else(
            || format!("参数 {}", field.input),
            |index| format!("参数 {} 的第 {} 项", field.input, index + 1),
        );
        CommandError::new(
            "invalid_arguments",
            format!("{location}不是合法 Cubism ID：{message}"),
        )
    })
}

pub(super) fn normalize_arguments(
    spec: &ToolSpec,
    args: Value,
    model_uid: Option<&str>,
) -> Result<Value, CommandError> {
    let args = args
        .as_object()
        .ok_or_else(|| CommandError::new("invalid_arguments", "工具参数必须是 JSON 对象。"))?;
    let allowed = spec
        .fields
        .iter()
        .map(|field| field.input)
        .collect::<BTreeSet<_>>();
    if let Some(unknown) = args.keys().find(|key| !allowed.contains(key.as_str())) {
        return Err(CommandError::new(
            "invalid_arguments",
            format!("未知参数 {unknown}。"),
        ));
    }
    let mut data = Map::new();
    if spec.uses_model {
        data.insert(
            "ModelUID".into(),
            json!(model_uid.ok_or_else(|| {
                CommandError::new("missing_model", "当前没有可用模型。")
            })?),
        );
    }
    for field in &spec.fields {
        match args.get(field.input) {
            Some(value) => {
                if !field.editor.is_empty() {
                    data.insert(field.editor.into(), validate_value(field, value)?);
                }
            }
            None if field.required => {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!("缺少参数 {}。", field.input),
                ));
            }
            None => {}
        }
    }
    Ok(Value::Object(data))
}

pub(super) fn normalize_operations(
    spec: &ToolSpec,
    args: Value,
    model_uid: &str,
) -> Result<Vec<(Value, Value)>, CommandError> {
    let args = args
        .as_object()
        .ok_or_else(|| CommandError::new("invalid_arguments", "工具参数必须是 JSON 对象。"))?;
    if args.keys().any(|key| key != "operations") {
        return Err(CommandError::new(
            "invalid_arguments",
            "官方编辑工具只接受 operations 数组。",
        ));
    }
    let operations = args
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| CommandError::new("invalid_arguments", "operations 必须是数组。"))?;
    if operations.is_empty() || operations.len() > MAX_BATCH_SIZE {
        return Err(CommandError::new(
            "invalid_arguments",
            format!("operations 必须包含 1 到 {MAX_BATCH_SIZE} 项。"),
        ));
    }
    operations
        .iter()
        .map(|operation| {
            let normalized = normalize_arguments(spec, operation.clone(), Some(model_uid))?;
            Ok((operation.clone(), normalized))
        })
        .collect()
}
