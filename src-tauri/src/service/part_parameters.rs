use super::model_structure::fetch_structure;
use super::CommandError;
use crate::domain::{
    ExistingParameter, ParameterGroupSummary, PartAssociatedObject, PartAssociatedParameter,
    PartParameterQueryResult, PartSelectionSummary,
};
use crate::protocol::{RpcClient, RpcError};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_CONCURRENT_KEY_REQUESTS: usize = 8;
const SUPPORTED_OBJECT_TYPES: [&str; 6] = [
    "ArtMesh",
    "WarpDeformer",
    "RotationDeformer",
    "Part",
    "ArtPath",
    "Glue",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartNode {
    id: String,
    name: String,
    object_type: String,
    children: Vec<PartNode>,
}

#[derive(Debug, Clone, PartialEq)]
struct ParameterKeys {
    id: String,
    key_values: Vec<f64>,
}

#[derive(Debug, Clone)]
struct ObjectMembership {
    id: String,
    name: String,
    object_type: String,
    source_part_ids: BTreeSet<String>,
}

struct ParameterAccumulator {
    metadata: ExistingParameter,
    group: Option<ParameterGroupSummary>,
    key_values: Vec<f64>,
    objects: Vec<PartAssociatedObject>,
}

pub(super) async fn find_selected(
    rpc: &RpcClient,
    model_uid: &str,
    model_label: &str,
) -> Result<PartParameterQueryResult, CommandError> {
    let (selected_response, part_response, parameter_structure) = tokio::try_join!(
        rpc.request("GetSelectedObjecs", json!({ "ModelUID": model_uid })),
        rpc.request("GetPartStructure", json!({ "ModelUID": model_uid })),
        fetch_structure(rpc, model_uid),
    )
    .map_err(CommandError::from)?;

    let selected_ids = parse_selected_ids(&selected_response).map_err(CommandError::from)?;
    let root = parse_part_structure(&part_response).map_err(CommandError::from)?;
    let mut node_index = BTreeMap::new();
    index_nodes(&root, &mut node_index).map_err(CommandError::from)?;

    let (selected_parts, ignored_selection_count) =
        resolve_selected_parts(selected_ids, &node_index);

    if selected_parts.is_empty() {
        return Err(CommandError::new(
            "no_selected_part",
            "请在 Cubism Editor 中选择至少一个 Part。",
        ));
    }

    let mut objects = BTreeMap::new();
    for part in &selected_parts {
        let node = node_index.get(&part.id).ok_or_else(|| {
            CommandError::new("model_changed", "模型在查询期间发生变化，请重试。")
        })?;
        collect_membership(node, &part.id, &mut objects);
    }

    let scanned_object_count = objects.len();
    let object_list: Vec<_> = objects.into_values().collect();
    let responses: Vec<_> = stream::iter(object_list)
        .map(|object| {
            let rpc = rpc.clone();
            let model_uid = model_uid.to_string();
            async move {
                let response = rpc
                    .request(
                        "GetParameterKeys",
                        json!({ "ModelUID": model_uid, "ObjectId": object.id }),
                    )
                    .await?;
                let keys = parse_parameter_keys(&response)?;
                Ok::<_, RpcError>((object, keys))
            }
        })
        .buffer_unordered(MAX_CONCURRENT_KEY_REQUESTS)
        .try_collect()
        .await
        .map_err(CommandError::from)?;

    let parameter_index: BTreeMap<_, _> = parameter_structure
        .parameters
        .into_iter()
        .map(|parameter| (parameter.id.clone(), parameter))
        .collect();
    let group_index: BTreeMap<_, _> = parameter_structure
        .groups
        .into_iter()
        .map(|group| (group.id.clone(), group))
        .collect();
    let mut parameters: BTreeMap<String, ParameterAccumulator> = BTreeMap::new();

    for (object, keys) in responses {
        for mut key in keys {
            normalize_key_values(&mut key.key_values);
            let metadata = parameter_index.get(&key.id).ok_or_else(|| {
                CommandError::new("model_changed", "模型在查询期间发生变化，请重试。")
            })?;
            let group = match metadata.group_id.as_ref() {
                Some(group_id) => Some(group_index.get(group_id).cloned().ok_or_else(|| {
                    CommandError::new("model_changed", "模型在查询期间发生变化，请重试。")
                })?),
                None => None,
            };
            let associated_object = PartAssociatedObject {
                id: object.id.clone(),
                name: object.name.clone(),
                object_type: object.object_type.clone(),
                key_values: key.key_values.clone(),
                source_part_ids: object.source_part_ids.iter().cloned().collect(),
            };
            let entry = parameters
                .entry(key.id.clone())
                .or_insert_with(|| ParameterAccumulator {
                    metadata: metadata.clone(),
                    group,
                    key_values: Vec::new(),
                    objects: Vec::new(),
                });
            entry.key_values.extend(key.key_values);
            entry.objects.push(associated_object);
        }
    }

    let mut parameters: Vec<_> = parameters
        .into_values()
        .map(|mut parameter| {
            normalize_key_values(&mut parameter.key_values);
            parameter.objects.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then_with(|| left.id.cmp(&right.id))
            });
            PartAssociatedParameter {
                id: parameter.metadata.id,
                name: parameter.metadata.name,
                group: parameter.group,
                key_values: parameter.key_values,
                objects: parameter.objects,
            }
        })
        .collect();
    parameters.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(PartParameterQueryResult {
        model_label: model_label.into(),
        selected_parts,
        ignored_selection_count,
        scanned_object_count,
        parameters,
    })
}

fn parse_selected_ids(response: &Value) -> Result<BTreeSet<String>, RpcError> {
    response
        .get("Ids")
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::Protocol("GetSelectedObjecs 缺少 Ids".into()))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| RpcError::Protocol("GetSelectedObjecs 包含无效 ID".into()))
        })
        .collect()
}

fn parse_part_structure(response: &Value) -> Result<PartNode, RpcError> {
    let value = response
        .get("PartStructure")
        .ok_or_else(|| RpcError::Protocol("GetPartStructure 缺少 PartStructure".into()))?;
    parse_part_node(value)
}

fn parse_part_node(value: &Value) -> Result<PartNode, RpcError> {
    let object_type = required_string(value, "Type", "PartStructure")?;
    if !SUPPORTED_OBJECT_TYPES.contains(&object_type.as_str()) {
        return Err(RpcError::Protocol(format!(
            "PartStructure 包含不支持的对象类型 {object_type}"
        )));
    }
    let children = match value.get("Children") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(children)) => children
            .iter()
            .map(parse_part_node)
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(RpcError::Protocol("PartStructure 的 Children 无效".into())),
    };
    Ok(PartNode {
        id: required_string(value, "Id", "PartStructure")?,
        name: required_string(value, "Name", "PartStructure")?,
        object_type,
        children,
    })
}

fn index_nodes(node: &PartNode, index: &mut BTreeMap<String, PartNode>) -> Result<(), RpcError> {
    if index.insert(node.id.clone(), node.clone()).is_some() {
        return Err(RpcError::Protocol(format!(
            "PartStructure 包含重复对象 ID {}",
            node.id
        )));
    }
    for child in &node.children {
        index_nodes(child, index)?;
    }
    Ok(())
}

fn collect_membership(
    node: &PartNode,
    source_part_id: &str,
    objects: &mut BTreeMap<String, ObjectMembership>,
) {
    let entry = objects
        .entry(node.id.clone())
        .or_insert_with(|| ObjectMembership {
            id: node.id.clone(),
            name: node.name.clone(),
            object_type: node.object_type.clone(),
            source_part_ids: BTreeSet::new(),
        });
    entry.source_part_ids.insert(source_part_id.into());
    for child in &node.children {
        collect_membership(child, source_part_id, objects);
    }
}

fn resolve_selected_parts(
    selected_ids: BTreeSet<String>,
    node_index: &BTreeMap<String, PartNode>,
) -> (Vec<PartSelectionSummary>, usize) {
    let mut selected_parts = Vec::new();
    let mut ignored_selection_count = 0;
    for selected_id in selected_ids {
        match node_index.get(&selected_id) {
            Some(node) if node.object_type == "Part" => {
                selected_parts.push(PartSelectionSummary {
                    id: node.id.clone(),
                    name: node.name.clone(),
                });
            }
            _ => ignored_selection_count += 1,
        }
    }
    selected_parts.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    selected_parts.dedup_by(|left, right| left.id == right.id);
    (selected_parts, ignored_selection_count)
}

fn parse_parameter_keys(response: &Value) -> Result<Vec<ParameterKeys>, RpcError> {
    let parameters = response
        .get("Parameters")
        .and_then(Value::as_array)
        .ok_or_else(|| RpcError::Protocol("GetParameterKeys 缺少 Parameters".into()))?;
    let mut seen = BTreeSet::new();
    parameters
        .iter()
        .map(|parameter| {
            let id = required_string(parameter, "Id", "GetParameterKeys")?;
            if !seen.insert(id.clone()) {
                return Err(RpcError::Protocol(format!(
                    "GetParameterKeys 包含重复参数 ID {id}"
                )));
            }
            let key_values = parameter
                .get("KeyValues")
                .and_then(Value::as_array)
                .ok_or_else(|| RpcError::Protocol("GetParameterKeys 缺少 KeyValues".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_f64()
                        .ok_or_else(|| RpcError::Protocol("GetParameterKeys 包含无效键值".into()))
                })
                .collect::<Result<_, _>>()?;
            Ok(ParameterKeys { id, key_values })
        })
        .collect()
}

fn required_string(value: &Value, key: &str, context: &str) -> Result<String, RpcError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| RpcError::Protocol(format!("{context} 缺少 {key}")))
}

fn normalize_key_values(values: &mut Vec<f64>) {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| *left == *right);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    fn part_response() -> Value {
        json!({
            "PartStructure": {
                "Name": "Root",
                "Id": "Root",
                "Type": "Part",
                "Children": [
                    {
                        "Name": "Face",
                        "Id": "PartFace",
                        "Type": "Part",
                        "Children": [
                            { "Name": "Eye", "Id": "ArtEye", "Type": "ArtMesh" },
                            {
                                "Name": "Mouth",
                                "Id": "PartMouth",
                                "Type": "Part",
                                "Children": [
                                    { "Name": "Lip", "Id": "ArtLip", "Type": "ArtMesh", "Children": [] }
                                ]
                            }
                        ]
                    }
                ]
            }
        })
    }

    #[test]
    fn parses_recursive_part_structure_and_leaf_without_children() {
        let root = parse_part_structure(&part_response()).unwrap();
        assert_eq!(root.children[0].children.len(), 2);
        assert!(root.children[0].children[0].children.is_empty());
    }

    #[test]
    fn rejects_duplicate_object_ids() {
        let root = parse_part_structure(&json!({
            "PartStructure": {
                "Name": "Root", "Id": "Root", "Type": "Part", "Children": [
                    { "Name": "A", "Id": "Same", "Type": "Part" },
                    { "Name": "B", "Id": "Same", "Type": "ArtMesh" }
                ]
            }
        }))
        .unwrap();
        let mut index = BTreeMap::new();
        assert!(index_nodes(&root, &mut index).is_err());
    }

    #[test]
    fn merges_overlapping_selected_part_membership() {
        let root = parse_part_structure(&part_response()).unwrap();
        let mut index = BTreeMap::new();
        index_nodes(&root, &mut index).unwrap();
        let mut objects = BTreeMap::new();
        collect_membership(index.get("PartFace").unwrap(), "PartFace", &mut objects);
        collect_membership(index.get("PartMouth").unwrap(), "PartMouth", &mut objects);

        assert_eq!(objects.len(), 4);
        assert_eq!(
            objects["ArtLip"].source_part_ids,
            BTreeSet::from(["PartFace".into(), "PartMouth".into()])
        );
    }

    #[test]
    fn filters_non_part_selection_and_reports_ignored_count() {
        let root = parse_part_structure(&part_response()).unwrap();
        let mut index = BTreeMap::new();
        index_nodes(&root, &mut index).unwrap();
        let (parts, ignored) = resolve_selected_parts(
            BTreeSet::from(["PartFace".into(), "ArtEye".into(), "Missing".into()]),
            &index,
        );

        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].id, "PartFace");
        assert_eq!(ignored, 2);
    }

    #[test]
    fn parses_and_normalizes_parameter_keys() {
        let mut values = parse_parameter_keys(&json!({
            "Parameters": [{ "Id": "ParamAngleX", "KeyValues": [30, 0, -30, 0] }]
        }))
        .unwrap();
        normalize_key_values(&mut values[0].key_values);
        assert_eq!(values[0].key_values, vec![-30.0, 0.0, 30.0]);
    }

    async fn query_server(failing_object: Option<&'static str>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                let Message::Text(message) = message else {
                    continue;
                };
                let request: Value = serde_json::from_str(&message).unwrap();
                let method = request["Method"].as_str().unwrap();
                let object_id = request["Data"]["ObjectId"].as_str();
                let should_fail = method == "GetParameterKeys" && object_id == failing_object;
                let (response_type, data) = if should_fail {
                    ("Error", json!({ "ErrorType": "InvalidData" }))
                } else {
                    let data = match method {
                        "GetSelectedObjecs" => json!({ "Ids": ["PartFace", "ArtOutside"] }),
                        "GetPartStructure" => json!({
                            "PartStructure": part_response()["PartStructure"].clone()
                        }),
                        "GetParameterStructure" => json!({
                            "ParameterStructure": {
                                "Entries": [
                                    { "EntryType": "Parameter", "Id": "ParamOpacity", "Name": "Opacity", "Min": 0, "Default": 1, "Max": 1 },
                                    { "EntryType": "Parameter", "Id": "ParamEyeOpen", "Name": "Eye Open", "Min": 0, "Default": 1, "Max": 1 },
                                    { "EntryType": "Parameter", "Id": "ParamMouthOpen", "Name": "Mouth Open", "Min": 0, "Default": 0, "Max": 1 }
                                ]
                            }
                        }),
                        "GetParameterKeys" => match object_id.unwrap() {
                            "PartFace" => {
                                json!({ "Parameters": [{ "Id": "ParamOpacity", "KeyValues": [0, 1] }] })
                            }
                            "ArtEye" => {
                                json!({ "Parameters": [{ "Id": "ParamEyeOpen", "KeyValues": [0, 1] }] })
                            }
                            "PartMouth" => {
                                json!({ "Parameters": [{ "Id": "ParamMouthOpen", "KeyValues": [0, 1] }] })
                            }
                            "ArtLip" => {
                                json!({ "Parameters": [{ "Id": "ParamMouthOpen", "KeyValues": [0, 0.5, 1] }] })
                            }
                            value => panic!("unexpected object {value}"),
                        },
                        value => panic!("unexpected method {value}"),
                    };
                    ("Response", data)
                };
                let response = json!({
                    "Version": crate::domain::EDIT_API_VERSION,
                    "RequestId": request["RequestId"],
                    "Type": response_type,
                    "Method": method,
                    "Data": data,
                });
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .unwrap();
            }
        });
        port
    }

    #[tokio::test]
    async fn queries_multiple_objects_and_returns_aggregated_details() {
        let port = query_server(None).await;
        let rpc = RpcClient::connect(port).await.unwrap();
        let result = find_selected(&rpc, "private-model", "当前建模模型")
            .await
            .unwrap();
        rpc.close().await;

        assert_eq!(result.selected_parts[0].id, "PartFace");
        assert_eq!(result.ignored_selection_count, 1);
        assert_eq!(result.scanned_object_count, 4);
        assert_eq!(result.parameters.len(), 3);
        let mouth = result
            .parameters
            .iter()
            .find(|parameter| parameter.id == "ParamMouthOpen")
            .unwrap();
        assert_eq!(mouth.key_values, vec![0.0, 0.5, 1.0]);
        assert_eq!(mouth.objects.len(), 2);
    }

    #[tokio::test]
    async fn fails_the_entire_query_when_one_object_request_fails() {
        let port = query_server(Some("ArtEye")).await;
        let rpc = RpcClient::connect(port).await.unwrap();
        let error = find_selected(&rpc, "private-model", "当前建模模型")
            .await
            .unwrap_err();
        rpc.close().await;

        assert_eq!(error.code, "InvalidData");
    }
}
