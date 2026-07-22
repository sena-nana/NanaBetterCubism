use super::{read::sanitize_response, schema::normalize_arguments, *};
use crate::{
    domain::{
        EditorConnectionState, EditorEditOutcome, StoredEditorEditItem, StoredEditorEditPlan,
    },
    protocol::RpcClient,
};
use futures_util::{SinkExt, StreamExt};
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

async fn sequence_server(steps: Vec<(&'static str, Value, Value)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        for (expected_method, expected_data, response_data) in steps {
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["Method"], expected_method);
            if expected_method != "EditSendLog" {
                assert_eq!(request["Data"], expected_data);
            }
            socket
                .send(Message::Text(
                    json!({
                        "Version": "1.1.0",
                        "RequestId": request["RequestId"],
                        "Type": "Response",
                        "Method": expected_method,
                        "Data": response_data
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
    });
    port
}

async fn connected_service(port: u16) -> EditorService {
    let rpc = RpcClient::connect(port).await.unwrap();
    let service = EditorService::default();
    {
        let mut inner = service.inner.lock().await;
        inner.rpc = Some(rpc);
        inner.model_uid = Some("private-model".into());
        inner.generation = 7;
        inner.snapshot.state = EditorConnectionState::Ready;
        inner.snapshot.capabilities.official_api = true;
        inner.snapshot.capabilities.official_edit_api = true;
    }
    service
}

fn edit_transaction_steps(
    previews: Vec<Value>,
    current: Vec<Value>,
    method: &'static str,
    mutations: Vec<(Value, bool)>,
    after: Vec<Value>,
) -> Vec<(&'static str, Value, Value)> {
    let mut steps = previews
        .into_iter()
        .map(|snapshot| {
            (
                "GetParameterStructure",
                json!({"ModelUID": "private-model"}),
                snapshot,
            )
        })
        .collect::<Vec<_>>();
    steps.push((
        "GetCurrentModelUID",
        json!({}),
        json!({"ModelUID": "private-model"}),
    ));
    steps.extend(current.into_iter().map(|snapshot| {
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            snapshot,
        )
    }));
    steps.extend([
        (
            "NotifyUndoCancel",
            json!({"Enabled": true}),
            json!({"Accepted": true}),
        ),
        (
            "EditBegin",
            json!({"Silent": false}),
            json!({"Result": true}),
        ),
        ("EditSendLog", Value::Null, json!({})),
    ]);
    let total = mutations.len();
    for (index, (data, succeeds)) in mutations.into_iter().enumerate() {
        steps.push((method, data, json!({"Result": succeeds})));
        if !succeeds {
            steps.push(("EditEnd", json!({"Cancel": true}), json!({"Result": true})));
            return steps;
        }
        steps.push((
            "EditSendProgress",
            json!({"Value": (index + 1) as f64 / total as f64}),
            json!({}),
        ));
    }
    steps.push(("EditEnd", json!({"Cancel": false}), json!({"Result": true})));
    steps.extend(after.into_iter().map(|snapshot| {
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            snapshot,
        )
    }));
    steps
}

#[test]
fn catalog_covers_every_official_method_without_exposing_session_primitives() {
    let exposed = tool_specs()
        .iter()
        .map(|spec| spec.method)
        .collect::<BTreeSet<_>>();
    let expected = [
        "GetParameterValues",
        "SetParameterValues",
        "GetParameters",
        "GetParameterGroups",
        "GetDocuments",
        "GetDocument",
        "GetCurrentDocumentUID",
        "GetCurrentModelUID",
        "GetCurrentEditMode",
        "ClearParameterValues",
        "GetPhysicsInfo",
        "SendCubismLog",
        "NotifyPhysicsFileExported",
        "NotifyMocFileExported",
        "NotifyMotionFileExported",
        "NotifyMotionSyncFileExported",
        "NotifyChangeEditMode",
        "AddParameterKey",
        "DeleteParameterKey",
        "MoveParameterKey",
        "GetParameterKeys",
        "GetObjectsByParameterKeys",
        "GetParameterStructure",
        "AddParameter",
        "AddParameterGroup",
        "EditParameter",
        "EditParameterGroup",
        "DeleteParameter",
        "DeleteParameterGroup",
        "MoveParameter",
        "MoveParameterGroup",
        "GetSelectedObjects",
        "AddSelectedObjects",
        "ClearSelectedObjects",
        "GetPartStructure",
        "GetObject",
        "DeleteObject",
        "MoveObjectOnPartsPalette",
        "AddPart",
        "EditPart",
        "EditArtMesh",
        "EditGlue",
        "GetDeformerStructure",
        "AddRotationDeformer",
        "AddWarpDeformer",
        "EditRotationDeformer",
        "EditWarpDeformer",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(exposed, expected);
    let internal = [
        "RegisterPlugin",
        "GetIsApproval",
        "SetGlobalVersion",
        "GetIsEditApproval",
        "EditBegin",
        "EditEnd",
        "EditSendLog",
        "EditSendProgress",
        "NotifyUndoCancel",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert!(exposed.is_disjoint(&internal));
    assert_eq!(exposed.union(&internal).count(), 56);
}

#[test]
fn schemas_use_documented_ranges_and_reject_raw_uids() {
    let tools = tool_definitions();
    let serialized = serde_json::to_string(&tools).unwrap();
    assert!(!serialized.contains("ModelUID"));
    assert!(!serialized.contains("DocumentUID"));
    let warp = tools
        .iter()
        .find(|tool| tool["function"]["name"] == "preview_add_warp_deformer")
        .unwrap();
    assert_eq!(
        warp["function"]["parameters"]["properties"]["operations"]["minItems"],
        1
    );
    assert_eq!(
        warp["function"]["parameters"]["properties"]["operations"]["maxItems"],
        crate::domain::MAX_BATCH_SIZE
    );
    assert_eq!(
        warp["function"]["parameters"]["properties"]["operations"]["items"]["properties"]
            ["warpDivH"]["minimum"],
        2
    );
    assert_eq!(
        warp["function"]["parameters"]["properties"]["operations"]["items"]["properties"]
            ["bezierDivV"]["maximum"],
        100
    );
    for name in [
        "preview_add_part",
        "preview_add_rotation_deformer",
        "preview_add_warp_deformer",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool["function"]["name"] == name)
            .unwrap();
        assert!(
            tool["function"]["parameters"]["properties"]["operations"]["items"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("id"))
        );
    }
}

#[test]
fn every_preview_schema_requires_a_bounded_operations_array() {
    let tools = tool_definitions();
    let previews = tool_specs()
        .iter()
        .filter(|spec| spec.mode == ToolMode::Preview)
        .collect::<Vec<_>>();
    assert_eq!(previews.len(), 23);
    for spec in previews {
        let tool = tools
            .iter()
            .find(|tool| tool["function"]["name"] == spec.tool_name)
            .unwrap();
        let parameters = &tool["function"]["parameters"];
        assert_eq!(parameters["required"], json!(["operations"]));
        assert_eq!(parameters["properties"].as_object().unwrap().len(), 1);
        assert_eq!(parameters["properties"]["operations"]["minItems"], 1);
        assert_eq!(
            parameters["properties"]["operations"]["maxItems"],
            crate::domain::MAX_BATCH_SIZE
        );
    }
}

#[test]
fn batch_arguments_reject_legacy_empty_and_oversized_inputs() {
    let spec = spec("preview_edit_parameter").unwrap();
    for args in [json!({"id": "ParamA"}), json!({"operations": []})] {
        assert!(schema::normalize_operations(spec, args, "private-model").is_err());
    }
    let operations = (0..=crate::domain::MAX_BATCH_SIZE)
        .map(|index| json!({"id": format!("Param{index}")}))
        .collect::<Vec<_>>();
    assert!(
        schema::normalize_operations(spec, json!({"operations": operations}), "private-model")
            .is_err()
    );
}

#[test]
fn object_creation_arguments_require_stable_ids() {
    for name in [
        "preview_add_part",
        "preview_add_rotation_deformer",
        "preview_add_warp_deformer",
    ] {
        let spec = spec(name).unwrap();
        let error = schema::normalize_operations(
            spec,
            json!({"operations": [{"name": "缺少稳定 ID"}]}),
            "private-model",
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
    }
}

#[test]
fn batch_conflicts_reject_duplicate_stable_ids_and_unsafe_dependencies() {
    assert!(edit::validate_batch_conflicts(
        "AddPart",
        &[
            json!({"ModelUID": "model", "Id": "PartA", "Name": "A"}),
            json!({"ModelUID": "model", "Id": "PartA", "Name": "B"}),
        ],
    )
    .is_err());
    assert!(edit::validate_batch_conflicts(
        "EditParameter",
        &[
            json!({"ModelUID": "model", "Id": "ParamA", "Name": "A"}),
            json!({"ModelUID": "model", "Id": "ParamA", "Name": "B"}),
        ],
    )
    .is_err());
    assert!(edit::validate_batch_conflicts(
        "EditParameter",
        &[
            json!({"ModelUID": "model", "Id": "ParamA", "NewId": "ParamB"}),
            json!({"ModelUID": "model", "Id": "ParamB", "Name": "B"}),
        ],
    )
    .is_err());
    assert!(edit::validate_batch_conflicts(
        "DeleteParameterKey",
        &[
            json!({"ModelUID": "model", "ObjectId": "Part", "ParameterId": "ParamA"}),
            json!({"ModelUID": "model", "ObjectId": "Part", "ParameterId": "ParamA", "KeyValue": 1.0}),
        ],
    )
    .is_err());
}

#[test]
fn ordered_move_batch_computes_final_positions_in_operation_order() {
    let root_order = json!(["GroupA", "GroupB", "GroupC"]);
    let plan = StoredEditorEditPlan {
        preview_id: "preview".into(),
        generation: 1,
        model_uid: "model".into(),
        method: "MoveParameterGroup".into(),
        items: vec![
            StoredEditorEditItem {
                data: json!({"ModelUID": "model", "Id": "GroupA", "InsertIndex": 2}),
                precondition: json!({"rootOrder": root_order}),
            },
            StoredEditorEditItem {
                data: json!({"ModelUID": "model", "Id": "GroupB", "InsertIndex": 0}),
                precondition: json!({"rootOrder": ["GroupA", "GroupB", "GroupC"]}),
            },
        ],
    };

    let positions = transaction::expected_ordered_move_positions(&plan).unwrap();

    assert_eq!(positions.get("GroupB"), Some(&0));
    assert_eq!(positions.get("GroupC"), Some(&1));
    assert_eq!(positions.get("GroupA"), Some(&2));
}

#[test]
fn converts_domain_arguments_to_exact_editor_field_names() {
    let spec = spec("preview_edit_part").unwrap();
    let data = normalize_arguments(
        spec,
        json!({"id": "PartFace", "isExactMatch": true, "opacity": 50}),
        Some("private-model"),
    )
    .unwrap();
    assert_eq!(data["ModelUID"], "private-model");
    assert_eq!(data["Id"], "PartFace");
    assert_eq!(data["isExactMatch"], true);
    assert_eq!(data["Opacity"], 50.0);
}

#[test]
fn sanitizes_all_session_uids_recursively() {
    let sanitized = sanitize_response(json!({
        "DocumentUID": "private-document",
        "Views": [{"ModelUID": "private-model", "Name": "view"}],
        "Groups": [{"GroupUID": "private-group", "GroupName": "Face"}]
    }));
    let serialized = serde_json::to_string(&sanitized).unwrap();
    assert!(!serialized.contains("private-"));
    assert_eq!(sanitized["views"][0]["name"], "view");
    assert_eq!(sanitized["groups"][0]["groupName"], "Face");
}

#[tokio::test]
async fn sends_exact_stable_api_payload_with_backend_owned_model_uid() {
    let port = sequence_server(vec![(
        "SetParameterValues",
        json!({
            "ModelUID": "private-model",
            "Parameters": [{"Id": "ParamAngleX", "Value": 12.5}]
        }),
        json!({}),
    )])
    .await;
    let service = connected_service(port).await;
    let response = call_tool(
        &service,
        "set_parameter_values",
        json!({"parameters": [{"id": "ParamAngleX", "value": 12.5}]}),
    )
    .await
    .unwrap();
    assert_eq!(response, json!({}));
}

#[tokio::test]
async fn get_selected_objects_uses_official_method_name_and_returns_ids() {
    let port = sequence_server(vec![(
        "GetSelectedObjects",
        json!({"ModelUID": "private-model"}),
        json!({"Ids": ["PartFace", "ArtOutside"]}),
    )])
    .await;
    let service = connected_service(port).await;
    let response = call_tool(&service, "get_selected_objects", json!({}))
        .await
        .unwrap();
    let ids: Vec<&str> = response["ids"]
        .as_array()
        .expect("sanitized ids array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["PartFace", "ArtOutside"]);
}

#[tokio::test]
async fn maps_parameter_group_uids_to_connection_local_indexes() {
    let port = sequence_server(vec![
        (
            "GetParameters",
            json!({"ModelUID": "private-model"}),
            json!({
                "Parameters": [{
                    "Id": "ParamFace",
                    "Name": "Face",
                    "GroupUID": "private-group",
                    "Default": 0,
                    "Max": 1,
                    "Min": -1,
                    "Repeat": false,
                    "Type": 0
                }]
            }),
        ),
        (
            "GetParameterGroups",
            json!({"ModelUID": "private-model"}),
            json!({
                "Groups": [{
                    "GroupUID": "private-group",
                    "GroupName": "Face"
                }]
            }),
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let response = call_tool(&service, "get_parameters", json!({}))
        .await
        .unwrap();
    assert_eq!(response["parameters"][0]["groupIndex"], 0);
    assert!(!serde_json::to_string(&response)
        .unwrap()
        .contains("private-group"));
}

#[tokio::test]
async fn document_refs_are_connection_scoped_and_hide_document_uids() {
    let port = sequence_server(vec![
        (
            "GetDocuments",
            json!({}),
            json!({
                "PhysicsDocuments": [],
                "ModelingDocuments": [{
                    "DocumentUID": "private-document",
                    "DocumentFilePath": "C:/models/test.cmo3",
                    "Views": [{"ModelUID": "private-model"}]
                }],
                "AnimationDocuments": []
            }),
        ),
        (
            "GetDocument",
            json!({"DocumentUID": "private-document"}),
            json!({
                "ModelingDocuments": [{
                    "DocumentFilePath": "C:/models/test.cmo3",
                    "Views": [{"ModelUID": "private-model"}]
                }]
            }),
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let documents = call_tool(&service, "list_editor_documents", json!({}))
        .await
        .unwrap();
    let document_ref = documents["modelingDocuments"][0]["documentRef"]
        .as_str()
        .unwrap();
    assert!(!serde_json::to_string(&documents)
        .unwrap()
        .contains("private-document"));
    let document = call_tool(
        &service,
        "get_editor_document",
        json!({"documentRef": document_ref}),
    )
    .await
    .unwrap();
    assert_eq!(
        document["modelingDocuments"][0]["documentFilePath"],
        "C:/models/test.cmo3"
    );
    assert!(!serde_json::to_string(&document)
        .unwrap()
        .contains("private-model"));
}

#[tokio::test]
async fn reads_saved_current_modeling_document_without_exposing_uid() {
    let port = sequence_server(vec![
        (
            "GetCurrentDocumentUID",
            json!({}),
            json!({"DocumentUID": "private-document"}),
        ),
        (
            "GetDocument",
            json!({"DocumentUID": "private-document"}),
            json!({
                "ModelingDocuments": [{
                    "DocumentFilePath": "C:\\Models\\Nana.cmo3",
                    "Views": [{"ModelUID": "private-model"}]
                }]
            }),
        ),
    ])
    .await;
    let service = connected_service(port).await;

    let document = current_modeling_document(&service).await.unwrap();

    assert_eq!(document.document_path, "C:/Models/Nana.cmo3");
    assert_eq!(
        document.document_key,
        if cfg!(windows) {
            "c:/models/nana.cmo3"
        } else {
            "C:/Models/Nana.cmo3"
        }
    );
    assert!(!format!("{document:?}").contains("private-document"));
    assert!(!format!("{document:?}").contains("private-model"));
}

#[tokio::test]
async fn ignores_unsaved_and_non_modeling_current_documents() {
    for document in [
        json!({"ModelingDocuments": [{"DocumentFilePath": "", "Views": []}]}),
        json!({"AnimationDocuments": [{"DocumentFilePath": "C:/Models/Nana.can3"}]}),
    ] {
        let port = sequence_server(vec![
            (
                "GetCurrentDocumentUID",
                json!({}),
                json!({"DocumentUID": "private-document"}),
            ),
            (
                "GetDocument",
                json!({"DocumentUID": "private-document"}),
                document,
            ),
        ])
        .await;
        let service = connected_service(port).await;
        assert!(current_modeling_document(&service).await.is_none());
    }
}

#[tokio::test]
async fn ignores_current_document_while_editor_is_disconnected() {
    let service = EditorService::default();

    assert!(current_modeling_document(&service).await.is_none());
}

#[tokio::test]
async fn ignores_document_result_from_superseded_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        let mut received_tx = Some(received_tx);
        let mut release_rx = Some(release_rx);
        for index in 0..2 {
            let request = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let method = request["Method"].as_str().unwrap();
            if index == 1 {
                let _ = received_tx.take().unwrap().send(());
                let _ = release_rx.take().unwrap().await;
            }
            let data = if index == 0 {
                json!({"DocumentUID": "private-document"})
            } else {
                json!({"ModelingDocuments": [{"DocumentFilePath": "C:/Models/Nana.cmo3"}]})
            };
            socket
                .send(Message::Text(
                    json!({
                        "Version": "1.1.0",
                        "RequestId": request["RequestId"],
                        "Type": "Response",
                        "Method": method,
                        "Data": data,
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
    });
    let service = connected_service(port).await;
    let task_service = service.clone();
    let task = tokio::spawn(async move { current_modeling_document(&task_service).await });
    received_rx.await.unwrap();
    service.inner.lock().await.generation = 8;
    release_tx.send(()).unwrap();

    assert!(task.await.unwrap().is_none());
}

#[tokio::test]
async fn edit_preview_executes_documented_transaction_and_verifies_postcondition() {
    let before = json!({
        "ParameterStructure": {
            "Name": "Root",
            "Id": "Root",
            "Entries": [{
                "EntryType": "Parameter",
                "Name": "Old",
                "Id": "ParamFace",
                "Min": -1.0,
                "Default": 0.0,
                "Max": 1.0,
                "IsRepeat": false,
                "IsBlendShape": false,
                "KeyValues": []
            }]
        }
    });
    let after = json!({
        "ParameterStructure": {
            "Name": "Root",
            "Id": "Root",
            "Entries": [{
                "EntryType": "Parameter",
                "Name": "Face",
                "Id": "ParamFace",
                "Min": -1.0,
                "Default": 0.0,
                "Max": 1.0,
                "IsRepeat": true,
                "IsBlendShape": false,
                "KeyValues": []
            }]
        }
    });
    let port = sequence_server(edit_transaction_steps(
        vec![before.clone()],
        vec![before],
        "EditParameter",
        vec![(
            json!({
                "ModelUID": "private-model",
                "Id": "ParamFace",
                "Name": "Face",
                "IsRepeat": true
            }),
            true,
        )],
        vec![after],
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [{"id": "ParamFace", "name": "Face", "isRepeat": true}]}),
    )
    .await
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };
    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;
    assert_eq!(result.outcome, EditorEditOutcome::Committed);
    assert!(result.verification.is_some());
}

#[tokio::test]
async fn warp_batch_with_explicit_ids_previews_and_verifies_hierarchy() {
    let before = json!({
        "DeformerStructure": [
            {"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []},
            {"Id": "ArtMeshB", "Name": "B", "Type": "ArtMesh", "Children": []}
        ]
    });
    let after = json!({
        "DeformerStructure": [
            {
                "Id": "DeformerA", "Name": "A", "Type": "WarpDeformer",
                "Children": [{"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []}]
            },
            {
                "Id": "DeformerB", "Name": "B", "Type": "WarpDeformer",
                "Children": [{"Id": "ArtMeshB", "Name": "B", "Type": "ArtMesh", "Children": []}]
            }
        ]
    });
    let first = json!({
        "ModelUID": "private-model",
        "Id": "DeformerA",
        "Name": "A",
        "TargetObjectIds": ["ArtMeshA"],
        "Mode": "AsParent",
        "WarpDivH": 2,
        "WarpDivV": 2
    });
    let second = json!({
        "ModelUID": "private-model",
        "Id": "DeformerB",
        "Name": "B",
        "TargetObjectIds": ["ArtMeshB"],
        "Mode": "AsParent",
        "WarpDivH": 2,
        "WarpDivV": 2
    });
    let port = sequence_server(vec![
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            before.clone(),
        ),
        (
            "GetCurrentModelUID",
            json!({}),
            json!({"ModelUID": "private-model"}),
        ),
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            before,
        ),
        (
            "NotifyUndoCancel",
            json!({"Enabled": true}),
            json!({"Accepted": true}),
        ),
        (
            "EditBegin",
            json!({"Silent": false}),
            json!({"Result": true}),
        ),
        ("EditSendLog", Value::Null, json!({})),
        ("AddWarpDeformer", first.clone(), json!({"Result": true})),
        ("EditSendProgress", json!({"Value": 0.5}), json!({})),
        ("AddWarpDeformer", second.clone(), json!({"Result": true})),
        ("EditSendProgress", json!({"Value": 1.0}), json!({})),
        ("EditEnd", json!({"Cancel": false}), json!({"Result": true})),
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            after,
        ),
        (
            "GetObject",
            json!({"ModelUID": "private-model", "Id": "DeformerA"}),
            json!({"Data": {
                "Id": "DeformerA", "Name": "A", "WarpDivH": 2, "WarpDivV": 2
            }}),
        ),
        (
            "GetObject",
            json!({"ModelUID": "private-model", "Id": "DeformerB"}),
            json!({"Data": {
                "Id": "DeformerB", "Name": "B", "WarpDivH": 2, "WarpDivV": 2
            }}),
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_add_warp_deformer",
        json!({"operations": [
            {
                "id": "DeformerA", "name": "A", "targetObjectIds": ["ArtMeshA"],
                "mode": "AsParent", "warpDivH": 2, "warpDivV": 2
            },
            {
                "id": "DeformerB", "name": "B", "targetObjectIds": ["ArtMeshB"],
                "mode": "AsParent", "warpDivH": 2, "warpDivV": 2
            }
        ]}),
    )
    .await
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Committed);
    assert_eq!(result.verification.unwrap().verified, 2);
}

#[tokio::test]
async fn changed_warp_target_hierarchy_stops_before_transaction() {
    let before = json!({"DeformerStructure": [
        {"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []}
    ]});
    let changed = json!({"DeformerStructure": [{
        "Id": "Existing", "Name": "Existing", "Type": "WarpDeformer", "Children": [
            {"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []}
        ]
    }]});
    let port = sequence_server(vec![
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            before,
        ),
        (
            "GetCurrentModelUID",
            json!({}),
            json!({"ModelUID": "private-model"}),
        ),
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            changed,
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_add_warp_deformer",
        json!({"operations": [{
            "id": "DeformerA", "name": "A", "targetObjectIds": ["ArtMeshA"],
            "mode": "AsParent"
        }]}),
    )
    .await
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Failed);
    assert_eq!(
        result.failure_code.as_deref(),
        Some("precondition_conflict")
    );
    assert_eq!(result.completed, 0);
}

#[tokio::test]
async fn mismatched_warp_hierarchy_reports_unknown_after_commit() {
    let before = json!({"DeformerStructure": [
        {"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []}
    ]});
    let after = json!({"DeformerStructure": [
        {"Id": "DeformerA", "Name": "A", "Type": "WarpDeformer", "Children": []},
        {"Id": "ArtMeshA", "Name": "A", "Type": "ArtMesh", "Children": []}
    ]});
    let data = json!({
        "ModelUID": "private-model",
        "Id": "DeformerA",
        "Name": "A",
        "TargetObjectIds": ["ArtMeshA"],
        "Mode": "AsParent"
    });
    let port = sequence_server(vec![
        (
            "GetCurrentModelUID",
            json!({}),
            json!({"ModelUID": "private-model"}),
        ),
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            before.clone(),
        ),
        (
            "NotifyUndoCancel",
            json!({"Enabled": true}),
            json!({"Accepted": true}),
        ),
        (
            "EditBegin",
            json!({"Silent": false}),
            json!({"Result": true}),
        ),
        ("EditSendLog", Value::Null, json!({})),
        ("AddWarpDeformer", data.clone(), json!({"Result": true})),
        ("EditSendProgress", json!({"Value": 1.0}), json!({})),
        ("EditEnd", json!({"Cancel": false}), json!({"Result": true})),
        (
            "GetDeformerStructure",
            json!({"ModelUID": "private-model"}),
            after,
        ),
        (
            "GetObject",
            json!({"ModelUID": "private-model", "Id": "DeformerA"}),
            json!({"Data": {"Id": "DeformerA", "Name": "A"}}),
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let rpc = service.inner.lock().await.rpc.clone().unwrap();
    let plan = StoredEditorEditPlan {
        preview_id: "preview".into(),
        generation: 7,
        model_uid: "private-model".into(),
        method: "AddWarpDeformer".into(),
        items: vec![StoredEditorEditItem {
            precondition: verification::edit_precondition("AddWarpDeformer", &data, &before)
                .unwrap(),
            data,
        }],
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Unknown);
    let verification = result.verification.unwrap();
    assert_eq!(verification.verified, 0);
    assert_eq!(verification.mismatched_indices, vec![1]);
}

#[tokio::test]
async fn same_type_batch_uses_one_transaction_and_verifies_every_item() {
    let before = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamA", "Name": "A"},
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "B"}
            ]
        }
    });
    let after = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamA", "Name": "Updated A"},
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "Updated B"}
            ]
        }
    });
    let first = json!({"ModelUID": "private-model", "Id": "ParamA", "Name": "Updated A"});
    let second = json!({"ModelUID": "private-model", "Id": "ParamB", "Name": "Updated B"});
    let port = sequence_server(edit_transaction_steps(
        vec![before.clone(), before.clone()],
        vec![before.clone(), before],
        "EditParameter",
        vec![(first, true), (second, true)],
        vec![after.clone(), after],
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [
            {"id": "ParamA", "name": "Updated A"},
            {"id": "ParamB", "name": "Updated B"}
        ]}),
    )
    .await
    .unwrap();
    assert_eq!(preview["operationCount"], 2);
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Committed);
    assert_eq!((result.completed, result.total), (2, 2));
    assert_eq!(result.verification.unwrap().verified, 2);
}

#[tokio::test]
async fn failed_batch_item_rolls_back_the_whole_transaction_once() {
    let before = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamA", "Name": "A"},
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "B"}
            ]
        }
    });
    let first = json!({"ModelUID": "private-model", "Id": "ParamA", "Name": "Updated A"});
    let second = json!({"ModelUID": "private-model", "Id": "ParamB", "Name": "Updated B"});
    let port = sequence_server(edit_transaction_steps(
        vec![before.clone(), before.clone()],
        vec![before.clone(), before],
        "EditParameter",
        vec![(first, true), (second, false)],
        vec![],
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [
            {"id": "ParamA", "name": "Updated A"},
            {"id": "ParamB", "name": "Updated B"}
        ]}),
    )
    .await
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::FailedRolledBack);
    assert_eq!((result.completed, result.total), (1, 2));
    assert!(result.verification.is_none());
}

#[tokio::test]
async fn official_edit_previews_from_the_same_model_snapshot_coexist() {
    let structure = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamAngleX", "Name": "Angle X"},
                {"EntryType": "Parameter", "Id": "ParamAngleY", "Name": "Angle Y"}
            ]
        }
    });
    let port = sequence_server(vec![
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            structure.clone(),
        ),
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            structure,
        ),
    ])
    .await;
    let service = connected_service(port).await;

    let first = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [{"id": "ParamAngleX", "name": "Angle X"}]}),
    )
    .await
    .unwrap();
    let second = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [{"id": "ParamAngleY", "name": "Angle Y"}]}),
    )
    .await
    .unwrap();
    let first_id = first["previewId"].as_str().unwrap();
    let second_id = second["previewId"].as_str().unwrap();
    let inner = service.inner.lock().await;

    assert_ne!(first_id, second_id);
    assert!(inner.editor_edit_previews.contains(first_id));
    assert!(inner.editor_edit_previews.contains(second_id));
}

#[tokio::test]
async fn pending_parameter_edit_executes_after_unrelated_parameter_deletion() {
    let before = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamA", "Name": "A"},
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "B"}
            ]
        }
    });
    let after_delete = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "B"}
            ]
        }
    });
    let after_edit = json!({
        "ParameterStructure": {
            "Entries": [
                {"EntryType": "Parameter", "Id": "ParamB", "Name": "Updated"}
            ]
        }
    });
    let port = sequence_server(edit_transaction_steps(
        vec![before],
        vec![after_delete],
        "EditParameter",
        vec![(
            json!({
                "ModelUID": "private-model",
                "Id": "ParamB",
                "Name": "Updated"
            }),
            true,
        )],
        vec![after_edit],
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"operations": [{"id": "ParamB", "name": "Updated"}]}),
    )
    .await
    .unwrap();
    let preview_id = preview["previewId"].as_str().unwrap();
    let (rpc, plan) = {
        let mut inner = service.inner.lock().await;
        (
            inner.rpc.clone().unwrap(),
            inner.editor_edit_previews.remove(preview_id).unwrap(),
        )
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Committed);
    assert_eq!(result.failure_code, None);
    assert!(result.verification.is_some());
}

#[tokio::test]
async fn changed_precondition_stops_before_editor_transaction() {
    let before = json!({
        "ParameterStructure": {
            "Entries": [{
                "EntryType": "Parameter",
                "Id": "ParamAngleX",
                "Name": "Angle X",
                "Min": -30,
                "Default": 0,
                "Max": 30
            }]
        }
    });
    let changed = json!({
        "ParameterStructure": {
            "Entries": [{
                "EntryType": "Parameter",
                "Id": "ParamAngleX",
                "Name": "Changed",
                "Min": -30,
                "Default": 0,
                "Max": 30
            }]
        }
    });
    let port = sequence_server(vec![
        (
            "GetCurrentModelUID",
            json!({}),
            json!({"ModelUID": "private-model"}),
        ),
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            changed,
        ),
    ])
    .await;
    let service = connected_service(port).await;
    let rpc = service.inner.lock().await.rpc.clone().unwrap();
    let data = json!({
        "ModelUID": "private-model",
        "Id": "ParamAngleX",
        "Name": "Updated"
    });
    let plan = StoredEditorEditPlan {
        preview_id: "preview".into(),
        generation: 7,
        model_uid: "private-model".into(),
        method: "EditParameter".into(),
        items: vec![StoredEditorEditItem {
            precondition: verification::edit_precondition("EditParameter", &data, &before).unwrap(),
            data,
        }],
    };

    let result = service
        .run_editor_edit_inner(&rpc, &plan, &AtomicBool::new(false))
        .await;

    assert_eq!(result.outcome, EditorEditOutcome::Failed);
    assert_eq!(
        result.failure_code.as_deref(),
        Some("precondition_conflict")
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["failureCode"],
        "precondition_conflict"
    );
    assert!(result.verification.is_none());
}

#[tokio::test]
async fn preview_rejects_a_missing_parameter_target() {
    let port = sequence_server(vec![(
        "GetParameterStructure",
        json!({"ModelUID": "private-model"}),
        json!({"ParameterStructure": {"Entries": []}}),
    )])
    .await;
    let service = connected_service(port).await;

    let error = call_tool(
        &service,
        "preview_delete_parameter",
        json!({"operations": [{"id": "Missing"}]}),
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "invalid_arguments");
}
