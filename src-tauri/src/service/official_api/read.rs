use super::{
    schema::{
        boolean, choice, effect, limited_string, normalize_arguments, number, parameter_filters,
        parameter_values, query, string, strings, ToolSpec, LOG_TYPES,
    },
    CommandError, CurrentModelingDocument, EditorService,
};
use crate::protocol::{RpcClient, RpcError};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

pub(crate) async fn current_modeling_document(
    service: &EditorService,
) -> Option<CurrentModelingDocument> {
    let (rpc, generation) = {
        let inner = service.inner.lock().await;
        if !inner.snapshot.capabilities.official_api {
            return None;
        }
        (inner.rpc.clone()?, inner.generation)
    };
    let current = rpc.request("GetCurrentDocumentUID", json!({})).await.ok()?;
    let document_uid = current.get("DocumentUID")?.as_str()?;
    let document = rpc
        .request("GetDocument", json!({"DocumentUID": document_uid}))
        .await
        .ok()?;
    let document_path = document
        .get("ModelingDocuments")?
        .as_array()?
        .iter()
        .find_map(|item| item.get("DocumentFilePath")?.as_str())?;
    let detected = normalize_modeling_document_path(document_path, document_uid)?;

    let inner = service.inner.lock().await;
    (inner.generation == generation && inner.rpc.is_some()).then_some(detected)
}

fn normalize_modeling_document_path(
    value: &str,
    document_uid: &str,
) -> Option<CurrentModelingDocument> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let document_path = trimmed.replace('\\', "/");
    let file_name = document_path.rsplit('/').next()?;
    if !file_name.to_ascii_lowercase().ends_with(".cmo3") {
        return None;
    }
    let document_key = if cfg!(windows) {
        document_path.to_lowercase()
    } else {
        document_path.clone()
    };
    Some(CurrentModelingDocument {
        document_instance_key: format!("{:x}", Sha256::digest(document_uid.as_bytes())),
        document_key,
        document_path,
    })
}

pub(super) fn specs() -> Vec<ToolSpec> {
    vec![
        query(
            "get_parameter_values",
            "读取参数值",
            "GetParameterValues",
            "读取当前模型的参数值；不包含动画信息。",
            true,
            vec![strings("ids", "Ids", false)],
        ),
        effect(
            "set_parameter_values",
            "设置临时参数值",
            "SetParameterValues",
            "向当前模型的 Editor 临时参数缓冲区写入参数值。",
            true,
            vec![parameter_values("parameters", "Parameters", true)],
        ),
        query(
            "get_parameters",
            "读取参数定义",
            "GetParameters",
            "读取当前模型参数及关键点定义。",
            true,
            vec![],
        ),
        query(
            "get_parameter_groups",
            "读取参数组",
            "GetParameterGroups",
            "读取当前模型的参数组。",
            true,
            vec![],
        ),
        query(
            "list_editor_documents",
            "列出 Editor 文档",
            "GetDocuments",
            "列出 Editor 当前打开的建模、物理和动画文档。",
            false,
            vec![],
        ),
        query(
            "get_editor_document",
            "读取 Editor 文档",
            "GetDocument",
            "按 list_editor_documents 返回的 documentRef 读取文档。",
            false,
            vec![string("documentRef", "", true)],
        ),
        query(
            "get_current_document",
            "读取当前文档",
            "GetCurrentDocumentUID",
            "读取 Editor 当前文档信息。",
            false,
            vec![],
        ),
        query(
            "get_current_model",
            "检查当前模型",
            "GetCurrentModelUID",
            "确认 Editor 当前是否有模型。",
            false,
            vec![],
        ),
        query(
            "get_current_edit_mode",
            "读取编辑模式",
            "GetCurrentEditMode",
            "读取 Editor 当前编辑模式。",
            false,
            vec![],
        ),
        effect(
            "clear_parameter_values",
            "清除临时参数值",
            "ClearParameterValues",
            "清除当前模型由外部应用写入的临时参数值。",
            true,
            vec![],
        ),
        query(
            "get_physics_info",
            "读取物理设置",
            "GetPhysicsInfo",
            "访问物理设置编辑器的计算 FPS 接口。",
            true,
            vec![number("fps", "Fps", false, Some(0.0), None)],
        ),
        effect(
            "send_cubism_log",
            "发送 Cubism 日志",
            "SendCubismLog",
            "向 Cubism Editor 日志面板发送一条日志。",
            false,
            vec![
                choice("type", "Type", false, LOG_TYPES),
                limited_string("message", "Message", true, 5000),
                boolean("display", "Display", false),
            ],
        ),
        effect(
            "notify_physics_file_exported",
            "监听物理文件导出",
            "NotifyPhysicsFileExported",
            "启用或停用物理设置文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        effect(
            "notify_moc_file_exported",
            "监听 MOC3 文件导出",
            "NotifyMocFileExported",
            "启用或停用 MOC3 及相关文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        effect(
            "notify_motion_file_exported",
            "监听 Motion 文件导出",
            "NotifyMotionFileExported",
            "启用或停用 motion 文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        effect(
            "notify_motion_sync_file_exported",
            "监听 MotionSync 文件导出",
            "NotifyMotionSyncFileExported",
            "启用或停用 motion-sync 文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        effect(
            "notify_change_edit_mode",
            "监听编辑模式变化",
            "NotifyChangeEditMode",
            "启用或停用 Editor 模式切换通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        query(
            "get_parameter_keys",
            "读取参数关键点",
            "GetParameterKeys",
            "读取对象关联参数的关键点值。",
            true,
            vec![string("objectId", "ObjectId", true)],
        ),
        query(
            "get_objects_by_parameter_key",
            "读取关键点关联对象",
            "GetObjectsByParameterKeys",
            "读取与指定参数关键点关联的对象。",
            true,
            vec![
                string("parameterId", "ParameterId", true),
                number("keyValue", "KeyValue", true, None, None),
            ],
        ),
        query(
            "get_parameter_structure",
            "读取参数结构",
            "GetParameterStructure",
            "读取当前模型的完整参数与参数组结构。",
            true,
            vec![],
        ),
        query(
            "get_selected_objects",
            "读取已选对象",
            "GetSelectedObjects",
            "读取当前模型已选择的对象 ID。",
            true,
            vec![],
        ),
        query(
            "get_part_structure",
            "读取 Part 结构",
            "GetPartStructure",
            "读取当前模型的 Part 与对象层级。",
            true,
            vec![],
        ),
        query(
            "get_object",
            "读取对象属性",
            "GetObject",
            "读取 Part、ArtMesh、Glue 或 Deformer 属性。id 必须使用结构化读取结果中的精确值，不能使用显示名称 name 或猜测名称映射。",
            true,
            vec![
                string("id", "Id", true),
                parameter_filters("parameters", "Parameters", false),
            ],
        ),
        query(
            "get_deformer_structure",
            "读取 Deformer 结构",
            "GetDeformerStructure",
            "读取当前模型的 Deformer 层级。",
            true,
            vec![],
        ),
    ]
}
fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_lowercase().chain(chars).collect()
}

pub(super) fn sanitize_response(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "Token" | "ModelUID" | "DocumentUID" | "GroupUID"
                    )
                })
                .map(|(key, value)| (lower_first(&key), sanitize_response(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(sanitize_response).collect()),
        value => value,
    }
}

pub(super) async fn session(
    service: &EditorService,
    require_edit: bool,
) -> Result<(RpcClient, Option<String>, u64), CommandError> {
    let inner = service.inner.lock().await;
    if inner.operation.is_some() {
        return Err(CommandError::new(
            "operation_active",
            "已有 Editor 编辑事务正在执行。",
        ));
    }
    if require_edit && !inner.snapshot.capabilities.official_edit_api {
        return Err(CommandError::new(
            "editor_not_ready",
            "当前 Editor 未授予编辑权限或不支持 1.1.0 编辑 API。",
        ));
    }
    if !require_edit && !inner.snapshot.capabilities.official_api {
        return Err(CommandError::new(
            "editor_not_ready",
            inner.snapshot.message.clone(),
        ));
    }
    Ok((
        inner
            .rpc
            .clone()
            .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?,
        inner.model_uid.clone(),
        inner.generation,
    ))
}

pub(super) async fn list_notifications(
    service: &EditorService,
    args: Value,
) -> Result<Value, CommandError> {
    if !args.as_object().is_some_and(Map::is_empty) {
        return Err(CommandError::new(
            "invalid_arguments",
            "list_editor_notifications 不接受参数。",
        ));
    }
    let (rpc, _, _) = session(service, false).await?;
    let notifications = rpc
        .recent_events()
        .await
        .into_iter()
        .filter(|event| event.method != "__Disconnected")
        .map(|event| {
            json!({
                "method": event.method,
                "data": sanitize_response(event.data)
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"notifications": notifications}))
}

async fn documents_with_refs(
    service: &EditorService,
    generation: u64,
    mut value: Value,
) -> Result<Value, CommandError> {
    let mut refs = HashMap::new();
    if let Some(object) = value.as_object_mut() {
        for key in [
            "PhysicsDocuments",
            "ModelingDocuments",
            "AnimationDocuments",
        ] {
            if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
                for item in items {
                    if let Some(item) = item.as_object_mut() {
                        if let Some(uid) = item.get("DocumentUID").and_then(Value::as_str) {
                            let document_ref = Uuid::new_v4().simple().to_string();
                            refs.insert(document_ref.clone(), uid.to_string());
                            item.insert("DocumentRef".into(), json!(document_ref));
                        }
                    }
                }
            }
        }
    }
    let mut inner = service.inner.lock().await;
    if inner.generation != generation {
        return Err(CommandError::new(
            "stale_document",
            "连接在列出文档期间发生变化，请重试。",
        ));
    }
    inner.document_refs = refs;
    Ok(value)
}

pub(super) async fn execute_direct(
    service: &EditorService,
    spec: &ToolSpec,
    args: Value,
) -> Result<Value, CommandError> {
    let (rpc, mut model_uid, generation) = session(service, false).await?;
    if spec.uses_model && model_uid.is_none() {
        let current = rpc
            .request("GetCurrentModelUID", json!({}))
            .await
            .map_err(CommandError::from)?;
        model_uid = Some(
            current
                .get("ModelUID")
                .and_then(Value::as_str)
                .ok_or_else(|| CommandError::new("protocol_error", "Editor 未返回当前模型。"))?
                .to_string(),
        );
    }
    if spec.method == "GetDocument" {
        normalize_arguments(spec, args.clone(), None)?;
        let document_ref = args
            .get("documentRef")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CommandError::new("invalid_arguments", "缺少 documentRef。"))?;
        let document_uid = service
            .inner
            .lock()
            .await
            .document_refs
            .get(document_ref)
            .cloned()
            .ok_or_else(|| {
                CommandError::new("stale_document", "文档引用已失效，请重新列出文档。")
            })?;
        return rpc
            .request("GetDocument", json!({"DocumentUID": document_uid}))
            .await
            .map(sanitize_response)
            .map_err(CommandError::from);
    }
    if spec.method == "GetCurrentDocumentUID" {
        normalize_arguments(spec, args, None)?;
        let current = rpc
            .request("GetCurrentDocumentUID", json!({}))
            .await
            .map_err(CommandError::from)?;
        let document_uid = current
            .get("DocumentUID")
            .and_then(Value::as_str)
            .ok_or_else(|| CommandError::new("protocol_error", "Editor 未返回当前文档。"))?;
        return rpc
            .request("GetDocument", json!({"DocumentUID": document_uid}))
            .await
            .map(sanitize_response)
            .map_err(CommandError::from);
    }
    if spec.method == "GetCurrentModelUID" {
        normalize_arguments(spec, args, None)?;
        rpc.request("GetCurrentModelUID", json!({}))
            .await
            .map_err(CommandError::from)?;
        return Ok(json!({"available": true}));
    }
    let data = normalize_arguments(spec, args, model_uid.as_deref())?;
    let mut response = rpc
        .request(spec.method, data)
        .await
        .map_err(|error| map_direct_error(spec.method, error))?;
    if spec.method == "GetDocuments" {
        response = documents_with_refs(service, generation, response).await?;
    } else if spec.method == "GetParameterGroups" {
        if let Some(groups) = response.get_mut("Groups").and_then(Value::as_array_mut) {
            for (index, group) in groups.iter_mut().enumerate() {
                if let Some(group) = group.as_object_mut() {
                    group.insert("GroupIndex".into(), json!(index));
                }
            }
        }
    } else if spec.method == "GetParameters" {
        let groups = rpc
            .request(
                "GetParameterGroups",
                json!({"ModelUID": model_uid.ok_or_else(|| {
                    CommandError::new("missing_model", "当前没有可用模型。")
                })?}),
            )
            .await
            .map_err(CommandError::from)?;
        let group_indexes = groups
            .get("Groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, group)| {
                group
                    .get("GroupUID")
                    .and_then(Value::as_str)
                    .map(|uid| (uid.to_string(), index))
            })
            .collect::<BTreeMap<_, _>>();
        if let Some(parameters) = response.get_mut("Parameters").and_then(Value::as_array_mut) {
            for parameter in parameters {
                if let Some(parameter) = parameter.as_object_mut() {
                    if let Some(index) = parameter
                        .get("GroupUID")
                        .and_then(Value::as_str)
                        .and_then(|uid| group_indexes.get(uid))
                    {
                        parameter.insert("GroupIndex".into(), json!(index));
                    }
                }
            }
        }
    }
    Ok(sanitize_response(response))
}

pub(super) fn map_direct_error(method: &str, error: RpcError) -> CommandError {
    if method == "GetObject" && error.editor_kind() == Some("InvalidData") {
        CommandError::new(
            "invalid_object_id",
            "对象 ID 无效。请重新读取 Part 或 Deformer 结构，并使用结构结果中的精确 id；不要使用显示名称或猜测映射。",
        )
    } else {
        CommandError::from(error)
    }
}
