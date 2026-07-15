use crate::domain::StoredEditorEditPlan;
use crate::protocol::{RpcClient, RpcError};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub(super) fn snapshot_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
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
        "AddParameter"
        | "AddParameterGroup"
        | "EditParameter"
        | "EditParameterGroup"
        | "DeleteParameter"
        | "DeleteParameterGroup"
        | "MoveParameter"
        | "MoveParameterGroup" => {
            rpc.request("GetParameterStructure", json!({"ModelUID": model_uid}))
                .await
        }
        "AddSelectedObjects" | "ClearSelectedObjects" => {
            rpc.request("GetSelectedObjecs", json!({"ModelUID": model_uid}))
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

pub(super) fn verify_postcondition(plan: &StoredEditorEditPlan, snapshot: &Value) -> Option<bool> {
    let data = &plan.data;
    match plan.method.as_str() {
        "AddParameter" | "AddParameterGroup" => {
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
        "AddPart" | "AddRotationDeformer" | "AddWarpDeformer" => {
            let id = data.get("Id")?.as_str()?;
            let actual = snapshot.get("Data")?.as_object()?;
            Some(
                actual.get("Id").and_then(Value::as_str) == Some(id)
                    && verify_fields(data, actual, &["ModelUID", "TargetObjectIds", "Mode"]),
            )
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
