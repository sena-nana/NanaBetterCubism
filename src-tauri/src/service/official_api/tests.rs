use super::{read::sanitize_response, schema::normalize_arguments, *};
use crate::{
    domain::{EditorConnectionState, EditorEditOutcome, StoredEditorEditPlan},
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
    preview: Value,
    current: Value,
    method: &'static str,
    data: Value,
    after: Value,
) -> Vec<(&'static str, Value, Value)> {
    vec![
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            preview,
        ),
        (
            "GetCurrentModelUID",
            json!({}),
            json!({"ModelUID": "private-model"}),
        ),
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            current,
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
        (method, data, json!({"Result": true})),
        ("EditSendProgress", json!({"Value": 1.0}), json!({})),
        ("EditEnd", json!({"Cancel": false}), json!({"Result": true})),
        (
            "GetParameterStructure",
            json!({"ModelUID": "private-model"}),
            after,
        ),
    ]
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
        warp["function"]["parameters"]["properties"]["warpDivH"]["minimum"],
        2
    );
    assert_eq!(
        warp["function"]["parameters"]["properties"]["bezierDivV"]["maximum"],
        100
    );
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
        before.clone(),
        before,
        "EditParameter",
        json!({
            "ModelUID": "private-model",
            "Id": "ParamFace",
            "Name": "Face",
            "IsRepeat": true
        }),
        after,
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"id": "ParamFace", "name": "Face", "isRepeat": true}),
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
        json!({"id": "ParamAngleX", "name": "Angle X"}),
    )
    .await
    .unwrap();
    let second = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"id": "ParamAngleY", "name": "Angle Y"}),
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
        before,
        after_delete,
        "EditParameter",
        json!({
            "ModelUID": "private-model",
            "Id": "ParamB",
            "Name": "Updated"
        }),
        after_edit,
    ))
    .await;
    let service = connected_service(port).await;
    let preview = call_tool(
        &service,
        "preview_edit_parameter",
        json!({"id": "ParamB", "name": "Updated"}),
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
        precondition: verification::edit_precondition("EditParameter", &data, &before).unwrap(),
        data,
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
        json!({"id": "Missing"}),
    )
    .await
    .unwrap_err();

    assert_eq!(error.code, "invalid_arguments");
}
