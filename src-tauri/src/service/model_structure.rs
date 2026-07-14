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
                let id = required_string(entry, "Id")?;
                let name = required_string(entry, "Name")?;
                structure.groups.push(ParameterGroupSummary {
                    id: id.clone(),
                    name,
                });
                if let Some(parameters) = entry.get("Parameters").and_then(Value::as_array) {
                    for parameter in parameters {
                        structure
                            .parameters
                            .push(parse_parameter(parameter, Some(id.clone()))?);
                    }
                }
            }
            Some("Parameter") => structure.parameters.push(parse_parameter(entry, None)?),
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

fn parse_parameter(value: &Value, group_id: Option<String>) -> Result<ExistingParameter, RpcError> {
    Ok(ExistingParameter {
        id: required_string(value, "Id")?,
        name: required_string(value, "Name")?,
        group_id,
        min: required_number(value, "Min")?,
        default: required_number(value, "Default")?,
        max: required_number(value, "Max")?,
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

fn required_string(value: &Value, key: &str) -> Result<String, RpcError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| RpcError::Protocol(format!("参数结构缺少 {key}")))
}

fn required_number(value: &Value, key: &str) -> Result<f64, RpcError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| RpcError::Protocol(format!("参数结构缺少 {key}")))
}

fn same_number(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON * 8.0
}
