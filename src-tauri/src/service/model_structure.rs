use crate::domain::{ExistingParameter, ModelStructure, ParameterGroupSummary, StoredPlan};
use crate::protocol::{RpcClient, RpcError};
use serde_json::{json, Value};

pub(super) async fn fetch_structure(
    rpc: &RpcClient,
    model_uid: &str,
) -> Result<ModelStructure, RpcError> {
    let response = rpc
        .request("GetParameterStructure", json!({ "ModelUID": model_uid }))
        .await?;
    parse_structure(&response)
}

pub(super) fn parse_structure(response: &Value) -> Result<ModelStructure, RpcError> {
    let entries = response
        .get("ParameterStructure")
        .and_then(|value| value.get("Entries"))
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::Protocol("GetParameterStructure 缺少 Entries".into()))?;
    let mut structure = ModelStructure::default();
    for entry in entries {
        match entry.get("EntryType").and_then(Value::as_str) {
            Some("ParameterGroup") => {
                let Some(id) = optional_string(entry, "Id") else {
                    continue;
                };
                let name = optional_string(entry, "Name").unwrap_or_else(|| id.clone());
                structure.groups.push(ParameterGroupSummary {
                    id: id.clone(),
                    name,
                });
                if let Some(parameters) = entry.get("Parameters").and_then(Value::as_array) {
                    for parameter in parameters {
                        match parse_parameter(parameter, Some(id.clone())) {
                            Some(parameter) => structure.parameters.push(parameter),
                            None => continue,
                        }
                    }
                }
            }
            Some("Parameter") => match parse_parameter(entry, None) {
                Some(parameter) => structure.parameters.push(parameter),
                None => continue,
            },
            _ => {}
        }
    }
    Ok(structure)
}

pub(super) fn verify_plan(plan: &StoredPlan, structure: &ModelStructure) -> bool {
    let group_ok = plan.new_group.as_ref().is_none_or(|group| {
        structure
            .groups
            .iter()
            .any(|actual| actual.id == group.id && actual.name == group.name)
    });
    group_ok
        && plan.rows.iter().all(|expected| {
            structure.parameters.iter().any(|actual| {
                actual.id == expected.id
                    && actual.name == expected.name
                    && actual.group_id == expected.group_id
                    && same_number(actual.min, expected.min)
                    && same_number(actual.default, expected.default)
                    && same_number(actual.max, expected.max)
                    && actual.is_blend_shape == expected.is_blend_shape
                    && actual.is_repeat == expected.is_repeat
            })
        })
}

/// Parse a parameter entry. Returns `None` when the model-owned `Id` is missing,
/// so the caller can skip the entry without aborting the whole structure parse.
/// `Name`/`Min`/`Default`/`Max` are optional per the capability matrix and fall
/// back to empty string / `0.0` when Editor omits them.
fn parse_parameter(value: &Value, group_id: Option<String>) -> Option<ExistingParameter> {
    let id = optional_string(value, "Id")?;
    Some(ExistingParameter {
        name: optional_string(value, "Name").unwrap_or_default(),
        min: optional_number(value, "Min").unwrap_or(0.0),
        default: optional_number(value, "Default").unwrap_or(0.0),
        max: optional_number(value, "Max").unwrap_or(0.0),
        id,
        group_id,
        is_blend_shape: value
            .get("IsBlendShape")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_repeat: value
            .get("IsRepeat")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn optional_number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn same_number(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON * 8.0
}
