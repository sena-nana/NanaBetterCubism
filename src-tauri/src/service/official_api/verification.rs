use crate::protocol::{RpcClient, RpcError};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

#[derive(Debug)]
pub(super) struct PreconditionError {
    pub(super) invalid_target: bool,
    pub(super) message: String,
}

impl PreconditionError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            invalid_target: true,
            message: message.into(),
        }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self {
            invalid_target: false,
            message: message.into(),
        }
    }
}

fn parameter_entries(snapshot: &Value) -> Result<&[Value], PreconditionError> {
    snapshot
        .get("ParameterStructure")
        .and_then(|value| value.get("Entries"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            PreconditionError::protocol(
                "GetParameterStructure 响应缺少 ParameterStructure.Entries。",
            )
        })
}

fn validate_entries(entries: &[Value]) -> Result<(), PreconditionError> {
    let mut ids = HashSet::new();
    for entry in entries {
        let entry_type = entry
            .get("EntryType")
            .and_then(Value::as_str)
            .ok_or_else(|| PreconditionError::protocol("参数结构条目缺少 EntryType。"))?;
        if !matches!(entry_type, "Parameter" | "ParameterGroup") {
            return Err(PreconditionError::protocol(format!(
                "无法解析参数结构条目类型 {entry_type}。"
            )));
        }
        let id = entry
            .get("Id")
            .and_then(Value::as_str)
            .ok_or_else(|| PreconditionError::protocol("参数结构条目缺少 Id。"))?;
        if !ids.insert(id) {
            return Err(PreconditionError::protocol(format!(
                "参数结构包含重复 ID {id}。"
            )));
        }
        if entry_type == "ParameterGroup" {
            let parameters = entry
                .get("Parameters")
                .and_then(Value::as_array)
                .ok_or_else(|| PreconditionError::protocol("参数组缺少 Parameters。"))?;
            for parameter in parameters {
                let id = parameter
                    .get("Id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PreconditionError::protocol("参数条目缺少 Id。"))?;
                if !ids.insert(id) {
                    return Err(PreconditionError::protocol(format!(
                        "参数结构包含重复 ID {id}。"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn find_group_entry<'a>(entries: &'a [Value], id: &str) -> Option<&'a Value> {
    entries.iter().find(|entry| {
        entry.get("EntryType").and_then(Value::as_str) == Some("ParameterGroup")
            && entry.get("Id").and_then(Value::as_str) == Some(id)
    })
}

fn find_parameter_state(entries: &[Value], id: &str) -> Option<(Value, Option<String>)> {
    for entry in entries {
        if entry.get("EntryType").and_then(Value::as_str) == Some("ParameterGroup") {
            let parent = entry.get("Id").and_then(Value::as_str).map(str::to_string);
            if let Some(parameter) =
                entry
                    .get("Parameters")
                    .and_then(Value::as_array)
                    .and_then(|parameters| {
                        parameters.iter().find(|parameter| {
                            parameter.get("Id").and_then(Value::as_str) == Some(id)
                        })
                    })
            {
                return Some((parameter.clone(), parent));
            }
        } else if entry.get("Id").and_then(Value::as_str) == Some(id) {
            return Some((entry.clone(), None));
        }
    }
    None
}

fn id_exists(entries: &[Value], id: &str) -> bool {
    entries.iter().any(|entry| {
        entry.get("Id").and_then(Value::as_str) == Some(id)
            || entry
                .get("Parameters")
                .and_then(Value::as_array)
                .is_some_and(|parameters| {
                    parameters
                        .iter()
                        .any(|parameter| parameter.get("Id").and_then(Value::as_str) == Some(id))
                })
    })
}

fn required_id<'a>(data: &'a Value, key: &str) -> Result<&'a str, PreconditionError> {
    data.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PreconditionError::invalid(format!("缺少 {key}。")))
}

fn require_parameter(
    entries: &[Value],
    id: &str,
) -> Result<(Value, Option<String>), PreconditionError> {
    find_parameter_state(entries, id)
        .ok_or_else(|| PreconditionError::invalid(format!("参数 {id} 不存在。")))
}

fn require_group(entries: &[Value], id: &str) -> Result<Value, PreconditionError> {
    find_group_entry(entries, id)
        .cloned()
        .ok_or_else(|| PreconditionError::invalid(format!("参数组 {id} 不存在。")))
}

fn group_metadata(group: &Value) -> Value {
    let mut metadata = group.clone();
    if let Some(object) = metadata.as_object_mut() {
        object.remove("Parameters");
    }
    metadata
}

fn parent_group(entries: &[Value], id: Option<&str>) -> Result<Value, PreconditionError> {
    id.map(|id| require_group(entries, id).map(|group| group_metadata(&group)))
        .unwrap_or(Ok(Value::Null))
}

fn direct_ids(container: &Value, field: &str) -> Result<Value, PreconditionError> {
    let values = container
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| PreconditionError::protocol(format!("参数结构缺少 {field}。")))?;
    values
        .iter()
        .map(|value| {
            value
                .get("Id")
                .and_then(Value::as_str)
                .map(|id| Value::String(id.to_string()))
                .ok_or_else(|| PreconditionError::protocol("参数结构条目缺少 Id。"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn ensure_id_available(
    entries: &[Value],
    candidate: Option<&str>,
    current: Option<&str>,
) -> Result<(), PreconditionError> {
    if let Some(id) = candidate.filter(|id| Some(*id) != current && id_exists(entries, id)) {
        return Err(PreconditionError::invalid(format!(
            "目标 ID {id} 已被占用。"
        )));
    }
    Ok(())
}

fn is_parameter_structure_method(method: &str) -> bool {
    matches!(
        method,
        "AddParameter"
            | "AddParameterGroup"
            | "EditParameter"
            | "EditParameterGroup"
            | "DeleteParameter"
            | "DeleteParameterGroup"
            | "MoveParameter"
            | "MoveParameterGroup"
    )
}

pub(super) fn edit_precondition(
    method: &str,
    data: &Value,
    snapshot: &Value,
) -> Result<Value, PreconditionError> {
    if !is_parameter_structure_method(method) {
        return Ok(snapshot.clone());
    }

    let entries = parameter_entries(snapshot)?;
    validate_entries(entries)?;
    match method {
        "AddParameter" | "AddParameterGroup" => {
            let target = required_id(data, "Id")?;
            ensure_id_available(entries, Some(target), None)?;
            if method == "AddParameter" {
                if let Some(id) = data
                    .get("GroupId")
                    .and_then(Value::as_str)
                    .filter(|id| find_group_entry(entries, id).is_none())
                {
                    return Err(PreconditionError::invalid(format!("参数组 {id} 不存在。")));
                }
            }
            Ok(Value::Null)
        }
        "EditParameter" | "DeleteParameter" => {
            let id = required_id(data, "Id")?;
            let (target, parent) = require_parameter(entries, id)?;
            if method == "EditParameter" {
                ensure_id_available(entries, data.get("NewId").and_then(Value::as_str), Some(id))?;
            }
            let parent = parent_group(entries, parent.as_deref())?;
            Ok(json!({"target": target, "parentGroup": parent}))
        }
        "EditParameterGroup" | "DeleteParameterGroup" => {
            let id = required_id(data, "Id")?;
            let target = require_group(entries, id)?;
            if method == "EditParameterGroup" {
                ensure_id_available(entries, data.get("NewId").and_then(Value::as_str), Some(id))?;
                Ok(json!({"target": group_metadata(&target)}))
            } else {
                Ok(json!({"target": target}))
            }
        }
        "MoveParameter" => {
            let id = required_id(data, "Id")?;
            let group_id = required_id(data, "GroupId")?;
            let (target, parent) = require_parameter(entries, id)?;
            let destination = require_group(entries, group_id)?;
            let source = parent_group(entries, parent.as_deref())?;
            let destination_order = if data.get("InsertIndex").is_some() {
                direct_ids(&destination, "Parameters")?
            } else {
                Value::Null
            };
            Ok(json!({
                "target": target,
                "sourceGroup": source,
                "destinationGroup": group_metadata(&destination),
                "destinationOrder": destination_order,
            }))
        }
        "MoveParameterGroup" => {
            let id = required_id(data, "Id")?;
            let target = require_group(entries, id)?;
            Ok(json!({
                "target": group_metadata(&target),
                "rootOrder": direct_ids(&snapshot["ParameterStructure"], "Entries")?,
            }))
        }
        _ => unreachable!(),
    }
}

pub(super) async fn verification_snapshot(
    rpc: &RpcClient,
    method: &str,
    data: &Value,
) -> Result<Value, RpcError> {
    let model_uid = data.get("ModelUID").cloned().unwrap_or(Value::Null);
    match method {
        "AddParameterKey" | "DeleteParameterKey" | "MoveParameterKey"
            if data.get("ObjectId").is_some() =>
        {
            rpc.request(
                "GetParameterKeys",
                json!({"ModelUID": model_uid, "ObjectId": data["ObjectId"]}),
            )
            .await
        }
        method if is_parameter_structure_method(method) => {
            rpc.request("GetParameterStructure", json!({"ModelUID": model_uid}))
                .await
        }
        "AddSelectedObjects" | "ClearSelectedObjects" => {
            rpc.request("GetSelectedObjects", json!({"ModelUID": model_uid}))
                .await
        }
        "DeleteObject" | "MoveObjectOnPartsPalette" => {
            rpc.request("GetPartStructure", json!({"ModelUID": model_uid}))
                .await
        }
        "AddPart" | "AddRotationDeformer" | "AddWarpDeformer" if data.get("Id").is_some() => {
            rpc.request(
                "GetObject",
                json!({"ModelUID": model_uid, "Id": data["Id"]}),
            )
            .await
        }
        "EditPart" | "EditArtMesh" | "EditGlue" | "EditRotationDeformer" | "EditWarpDeformer" => {
            let id = data
                .get("NewId")
                .or_else(|| data.get("Id"))
                .cloned()
                .unwrap_or(Value::Null);
            rpc.request("GetObject", json!({"ModelUID": model_uid, "Id": id}))
                .await
        }
        _ => Ok(Value::Null),
    }
}

fn contains_id(value: &Value, id: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.get("Id").and_then(Value::as_str) == Some(id)
                || object.values().any(|value| contains_id(value, id))
        }
        Value::Array(values) => values.iter().any(|value| contains_id(value, id)),
        _ => false,
    }
}

fn find_id<'a>(value: &'a Value, id: &str) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(object) => {
            if object.get("Id").and_then(Value::as_str) == Some(id) {
                return Some(object);
            }
            object.values().find_map(|value| find_id(value, id))
        }
        Value::Array(values) => values.iter().find_map(|value| find_id(value, id)),
        _ => None,
    }
}

fn same_scalar(left: &Value, right: &Value) -> bool {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => (left - right).abs() <= 1e-7,
        _ => left == right,
    }
}

fn verify_fields(data: &Value, actual: &Map<String, Value>, ignored: &[&str]) -> bool {
    data.as_object().is_some_and(|expected| {
        expected.iter().all(|(key, value)| {
            ignored.contains(&key.as_str())
                || actual
                    .get(key)
                    .is_some_and(|actual| same_scalar(value, actual))
        })
    })
}

fn direct_id_position(container: &Map<String, Value>, id: &str) -> Option<usize> {
    container
        .get("Parameters")
        .or_else(|| container.get("Entries"))
        .or_else(|| container.get("Children"))
        .and_then(Value::as_array)?
        .iter()
        .position(|value| value.get("Id").and_then(Value::as_str) == Some(id))
}

fn find_child_position<'a>(value: &'a Value, id: &str) -> Option<(&'a Map<String, Value>, usize)> {
    let object = value.as_object()?;
    if let Some(children) = object.get("Children").and_then(Value::as_array) {
        if let Some(index) = children
            .iter()
            .position(|child| child.get("Id").and_then(Value::as_str) == Some(id))
        {
            return Some((object, index));
        }
        for child in children {
            if let Some(found) = find_child_position(child, id) {
                return Some(found);
            }
        }
    }
    object
        .values()
        .find_map(|value| find_child_position(value, id))
}

/// Returns the parent group id of a parameter in `GetParameterStructure`:
/// `Some(None)` at root, `Some(Some(group_id))` nested under a group, `None` if absent.
fn find_parameter_parent<'a>(snapshot: &'a Value, id: &str) -> Option<Option<&'a str>> {
    let entries = snapshot
        .get("ParameterStructure")
        .and_then(|value| value.get("Entries"))
        .and_then(Value::as_array)?;
    for entry in entries {
        match entry.get("EntryType").and_then(Value::as_str) {
            Some("ParameterGroup") => {
                if let Some(parameters) = entry.get("Parameters").and_then(Value::as_array) {
                    if parameters
                        .iter()
                        .any(|p| p.get("Id").and_then(Value::as_str) == Some(id))
                    {
                        return Some(entry.get("Id").and_then(Value::as_str));
                    }
                }
            }
            Some("Parameter") if entry.get("Id").and_then(Value::as_str) == Some(id) => {
                return Some(None);
            }
            _ => {}
        }
    }
    None
}

pub(super) fn verify_postcondition(method: &str, data: &Value, snapshot: &Value) -> Option<bool> {
    match method {
        "AddParameter" => {
            let id = data.get("Id")?.as_str()?;
            let parent = find_parameter_parent(snapshot, id)?;
            let expected_group = data.get("GroupId").and_then(Value::as_str);
            let group_ok = match expected_group {
                Some(expected) => parent == Some(expected),
                None => parent.is_none(),
            };
            if !group_ok {
                return Some(false);
            }
            let actual = find_id(snapshot, id)?;
            Some(verify_fields(data, actual, &["ModelUID", "GroupId"]))
        }
        "AddParameterGroup" => {
            let id = data.get("Id")?.as_str()?;
            let actual = find_id(snapshot, id)?;
            Some(verify_fields(data, actual, &["ModelUID", "GroupId"]))
        }
        "EditParameter" | "EditParameterGroup" => {
            let id = data.get("NewId").or_else(|| data.get("Id"))?.as_str()?;
            let actual = find_id(snapshot, id)?;
            let mut mapped = data.clone();
            if let Some(object) = mapped.as_object_mut() {
                if let Some(new_id) = object.remove("NewId") {
                    object.insert("Id".into(), new_id);
                }
            }
            Some(verify_fields(&mapped, actual, &["ModelUID"]))
        }
        "DeleteParameter" | "DeleteParameterGroup" | "DeleteObject" => {
            Some(!contains_id(snapshot, data.get("Id")?.as_str()?))
        }
        "AddSelectedObjects" => {
            let selected = snapshot.get("Ids")?.as_array()?;
            let expected = data.get("Ids")?.as_array()?;
            Some(expected.iter().all(|id| selected.contains(id)))
        }
        "ClearSelectedObjects" => Some(snapshot.get("Ids")?.as_array()?.is_empty()),
        "AddParameterKey" => {
            let parameter_id = data.get("ParameterId")?.as_str()?;
            let key_value = data.get("KeyValue")?.as_f64()?;
            let parameter = find_id(snapshot, parameter_id)?;
            Some(
                parameter
                    .get("KeyValues")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_f64)
                    .any(|value| (value - key_value).abs() <= 1e-7),
            )
        }
        "DeleteParameterKey"
            if data.get("ObjectId").is_some() && data.get("ParameterId").is_some() =>
        {
            let parameter_id = data.get("ParameterId")?.as_str()?;
            let key_value = data.get("KeyValue").and_then(Value::as_f64);
            let Some(parameter) = find_id(snapshot, parameter_id) else {
                return Some(true);
            };
            let values = parameter.get("KeyValues")?.as_array()?;
            Some(match key_value {
                Some(key) => !values
                    .iter()
                    .filter_map(Value::as_f64)
                    .any(|value| (value - key).abs() <= 1e-7),
                None => values.is_empty(),
            })
        }
        "MoveParameterKey"
            if data.get("ObjectId").is_some() && data.get("ParameterId").is_some() =>
        {
            let parameter = find_id(snapshot, data.get("ParameterId")?.as_str()?)?;
            let values = parameter.get("KeyValues")?.as_array()?;
            let from = data.get("FromValue")?.as_f64()?;
            let to = data.get("ToValue")?.as_f64()?;
            Some(
                !values
                    .iter()
                    .filter_map(Value::as_f64)
                    .any(|value| (value - from).abs() <= 1e-7)
                    && values
                        .iter()
                        .filter_map(Value::as_f64)
                        .any(|value| (value - to).abs() <= 1e-7),
            )
        }
        "AddPart" => {
            let id = data.get("Id")?.as_str()?;
            let actual = snapshot.get("Data")?.as_object()?;
            Some(
                actual.get("Id").and_then(Value::as_str) == Some(id)
                    && verify_fields(data, actual, &["ModelUID", "TargetObjectIds", "Mode"]),
            )
        }
        "AddRotationDeformer" | "AddWarpDeformer" => {
            let id = data.get("Id")?.as_str()?;
            let actual = snapshot.get("Data")?.as_object()?;
            let base = actual.get("Id").and_then(Value::as_str) == Some(id)
                && verify_fields(data, actual, &["ModelUID", "TargetObjectIds", "Mode"]);
            if !base {
                return Some(false);
            }
            // GetObject's deformer Data shape for TargetObjectIds/Mode is not confirmed
            // in the capability matrix, so we cannot reliably verify those structural
            // fields. Admit Unknown rather than falsely reporting Committed.
            if data.get("TargetObjectIds").is_some() || data.get("Mode").is_some() {
                None
            } else {
                Some(true)
            }
        }
        "MoveParameter" => {
            let group = find_id(snapshot, data.get("GroupId")?.as_str()?)?;
            let position = direct_id_position(group, data.get("Id")?.as_str()?)?;
            Some(
                data.get("InsertIndex")
                    .and_then(Value::as_u64)
                    .is_none_or(|expected| position == expected as usize),
            )
        }
        "MoveParameterGroup" => {
            let root = snapshot.get("ParameterStructure")?.as_object()?;
            let position = direct_id_position(root, data.get("Id")?.as_str()?)?;
            Some(position == data.get("InsertIndex")?.as_u64()? as usize)
        }
        "MoveObjectOnPartsPalette" => {
            if data.get("InsertId").is_some() {
                return None;
            }
            let (parent, position) = find_child_position(snapshot, data.get("Id")?.as_str()?)?;
            let parent_matches = data
                .get("ParentId")
                .and_then(Value::as_str)
                .is_none_or(|expected| parent.get("Id").and_then(Value::as_str) == Some(expected));
            let index_matches = data
                .get("InsertIndex")
                .and_then(Value::as_u64)
                .is_none_or(|expected| position == expected as usize);
            Some(parent_matches && index_matches)
        }
        "EditPart" | "EditArtMesh" | "EditGlue" | "EditRotationDeformer" | "EditWarpDeformer" => {
            let actual = snapshot.get("Data")?.as_object()?;
            let mut mapped = data.clone();
            if let Some(object) = mapped.as_object_mut() {
                if let Some(new_id) = object.remove("NewId") {
                    object.insert("Id".into(), new_id);
                }
            }
            Some(verify_fields(
                &mapped,
                actual,
                &["ModelUID", "Parameters", "IsExactMatch", "isExactMatch"],
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn structure(entries: Value) -> Value {
        json!({ "ParameterStructure": { "Entries": entries } })
    }

    fn group(id: &str, name: &str, parameters: Vec<Value>) -> Value {
        json!({ "EntryType": "ParameterGroup", "Id": id, "Name": name, "Parameters": parameters })
    }

    fn parameter(id: &str, name: &str, root: bool) -> Value {
        let mut entry = json!({
            "Id": id,
            "Name": name,
            "Min": 0.0,
            "Default": 0.0,
            "Max": 1.0,
            "IsBlendShape": false,
        });
        if root {
            entry["EntryType"] = json!("Parameter");
        }
        entry
    }

    fn add_parameter_data(group_id: Option<&str>) -> Value {
        let mut data = json!({
            "ModelUID": "model",
            "Id": "ParamAngleX",
            "Name": "角度X",
            "Min": 0.0,
            "Default": 0.0,
            "Max": 1.0,
            "IsBlendShape": false,
        });
        if let Some(group_id) = group_id {
            data["GroupId"] = json!(group_id);
        }
        data
    }

    fn verify(snapshot: Value, group_id: Option<&str>) -> Option<bool> {
        verify_postcondition("AddParameter", &add_parameter_data(group_id), &snapshot)
    }

    #[test]
    fn add_parameter_with_group_id_matches_parent() {
        let snapshot = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![parameter("ParamAngleX", "角度X", false)]
        )]));
        assert_eq!(verify(snapshot, Some("FaceGroup")), Some(true));
    }

    #[test]
    fn add_parameter_with_group_id_lands_at_root_reports_mismatch() {
        let snapshot = structure(json!([
            group("FaceGroup", "脸部", vec![]),
            parameter("ParamAngleX", "角度X", true),
        ]));
        assert_eq!(verify(snapshot, Some("FaceGroup")), Some(false));
    }

    #[test]
    fn add_parameter_with_group_id_in_wrong_group_reports_mismatch() {
        let snapshot = structure(json!([
            group("FaceGroup", "脸部", vec![]),
            group(
                "BodyGroup",
                "身体",
                vec![parameter("ParamAngleX", "角度X", false)]
            ),
        ]));
        assert_eq!(verify(snapshot, Some("FaceGroup")), Some(false));
    }

    #[test]
    fn add_parameter_without_group_id_at_root_passes() {
        let snapshot = structure(json!([parameter("ParamAngleX", "角度X", true)]));
        assert_eq!(verify(snapshot, None), Some(true));
    }

    #[test]
    fn add_parameter_without_group_id_but_nested_reports_mismatch() {
        let snapshot = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![parameter("ParamAngleX", "角度X", false)]
        )]));
        assert_eq!(verify(snapshot, None), Some(false));
    }

    #[test]
    fn add_parameter_group_ignores_group_id_field() {
        let snapshot = structure(json!([group("FaceGroup", "脸部", vec![])]));
        let data = json!({ "ModelUID": "model", "Id": "FaceGroup", "Name": "脸部" });
        assert_eq!(
            verify_postcondition("AddParameterGroup", &data, &snapshot),
            Some(true)
        );
    }

    #[test]
    fn add_parameter_absent_from_snapshot_returns_none() {
        let snapshot = structure(json!([parameter("Other", "其他", true)]));
        assert_eq!(verify(snapshot, Some("FaceGroup")), None);
    }

    #[test]
    fn parameter_precondition_ignores_unrelated_deletion() {
        let before = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![
                parameter("ParamA", "A", false),
                parameter("ParamB", "B", false),
            ],
        )]));
        let after = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![parameter("ParamB", "B", false)],
        )]));
        let data = json!({"ModelUID": "model", "Id": "ParamB", "Name": "Updated"});

        assert_eq!(
            edit_precondition("EditParameter", &data, &before).unwrap(),
            edit_precondition("EditParameter", &data, &after).unwrap()
        );
    }

    #[test]
    fn parameter_precondition_detects_target_change_and_missing_target() {
        let before = structure(json!([parameter("ParamB", "B", true)]));
        let changed = structure(json!([parameter("ParamB", "Changed", true)]));
        let missing = structure(json!([]));
        let data = json!({"ModelUID": "model", "Id": "ParamB", "Name": "Updated"});

        assert_ne!(
            edit_precondition("EditParameter", &data, &before).unwrap(),
            edit_precondition("EditParameter", &data, &changed).unwrap()
        );
        assert!(
            edit_precondition("EditParameter", &data, &missing)
                .unwrap_err()
                .invalid_target
        );
    }

    #[test]
    fn group_delete_precondition_includes_approved_children() {
        let before = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![
                parameter("ParamA", "A", false),
                parameter("ParamB", "B", false)
            ],
        )]));
        let changed = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![parameter("ParamB", "B", false)],
        )]));
        let data = json!({"ModelUID": "model", "Id": "FaceGroup"});

        assert_ne!(
            edit_precondition("DeleteParameterGroup", &data, &before).unwrap(),
            edit_precondition("DeleteParameterGroup", &data, &changed).unwrap()
        );
    }

    #[test]
    fn rename_rejects_an_occupied_id() {
        let snapshot = structure(json!([
            parameter("ParamA", "A", true),
            parameter("ParamB", "B", true),
        ]));
        let data = json!({"ModelUID": "model", "Id": "ParamA", "NewId": "ParamB"});

        assert!(
            edit_precondition("EditParameter", &data, &snapshot)
                .unwrap_err()
                .invalid_target
        );
    }

    #[test]
    fn duplicate_ids_reject_the_snapshot() {
        let snapshot = structure(json!([
            parameter("ParamA", "A", true),
            group(
                "FaceGroup",
                "脸部",
                vec![parameter("ParamA", "Duplicate", false)],
            ),
        ]));
        let data = json!({"ModelUID": "model", "Id": "ParamA", "Name": "Updated"});

        let error = edit_precondition("EditParameter", &data, &snapshot).unwrap_err();
        assert!(!error.invalid_target);
    }

    #[test]
    fn indexed_move_precondition_tracks_destination_order() {
        let before = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![
                parameter("ParamA", "A", false),
                parameter("ParamB", "B", false)
            ],
        )]));
        let changed = structure(json!([group(
            "FaceGroup",
            "脸部",
            vec![parameter("ParamB", "B", false)],
        )]));
        let data = json!({
            "ModelUID": "model",
            "Id": "ParamB",
            "GroupId": "FaceGroup",
            "InsertIndex": 0,
        });

        assert_ne!(
            edit_precondition("MoveParameter", &data, &before).unwrap(),
            edit_precondition("MoveParameter", &data, &changed).unwrap()
        );
    }

    fn object_snapshot(data: Value) -> Value {
        json!({ "Data": data })
    }

    fn deformer_data(extras: Value) -> Value {
        let mut data = json!({ "ModelUID": "model", "Id": "Rotator", "Name": "旋转" });
        if let Some(object) = extras.as_object() {
            for (key, value) in object {
                data[key] = value.clone();
            }
        }
        data
    }

    #[test]
    fn add_rotation_deformer_with_target_object_ids_reports_unknown() {
        let snapshot = object_snapshot(json!({ "Id": "Rotator", "Name": "旋转" }));
        let data = deformer_data(json!({ "TargetObjectIds": ["ArtMesh1"] }));
        assert_eq!(
            verify_postcondition("AddRotationDeformer", &data, &snapshot),
            None
        );
    }

    #[test]
    fn add_rotation_deformer_with_mode_reports_unknown() {
        let snapshot = object_snapshot(json!({ "Id": "Rotator", "Name": "旋转" }));
        let data = deformer_data(json!({ "Mode": "AsChild" }));
        assert_eq!(
            verify_postcondition("AddRotationDeformer", &data, &snapshot),
            None
        );
    }

    #[test]
    fn add_rotation_deformer_without_structural_fields_passes() {
        let snapshot = object_snapshot(json!({ "Id": "Rotator", "Name": "旋转" }));
        let data = deformer_data(json!({}));
        assert_eq!(
            verify_postcondition("AddRotationDeformer", &data, &snapshot),
            Some(true)
        );
    }

    #[test]
    fn add_rotation_deformer_id_mismatch_reports_false() {
        let snapshot = object_snapshot(json!({ "Id": "Other", "Name": "旋转" }));
        let data = deformer_data(json!({}));
        assert_eq!(
            verify_postcondition("AddRotationDeformer", &data, &snapshot),
            Some(false)
        );
    }

    #[test]
    fn add_warp_deformer_with_target_object_ids_reports_unknown() {
        let snapshot = object_snapshot(
            json!({ "Id": "Rotator", "Name": "旋转", "WarpDivH": 2, "WarpDivV": 2 }),
        );
        let data =
            deformer_data(json!({ "TargetObjectIds": ["ArtMesh1"], "WarpDivH": 2, "WarpDivV": 2 }));
        assert_eq!(
            verify_postcondition("AddWarpDeformer", &data, &snapshot),
            None
        );
    }

    #[test]
    fn add_part_with_ids_still_verifies_fields() {
        let snapshot =
            object_snapshot(json!({ "Id": "PartA", "Name": "头部", "Ids": ["ArtMesh1"] }));
        let data =
            json!({ "ModelUID": "model", "Id": "PartA", "Name": "头部", "Ids": ["ArtMesh1"] });
        assert_eq!(
            verify_postcondition("AddPart", &data, &snapshot),
            Some(true)
        );
    }
}
