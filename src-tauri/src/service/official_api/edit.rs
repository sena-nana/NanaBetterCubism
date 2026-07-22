use super::{
    read::session,
    schema::{
        boolean, choice, color, common_object_edit_fields, integer, normalize_operations, number,
        preview, string, strings, ToolSpec, ALPHA_BLENDS, COLOR_BLENDS, DEFORMER_MODES,
        LABEL_COLORS,
    },
    transaction::expected_ordered_move_positions,
    verification::{edit_precondition, verification_snapshot},
    CommandError, EditorService,
};
use crate::domain::{EditorEditPreview, StoredEditorEditItem, StoredEditorEditPlan};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) fn specs() -> Vec<ToolSpec> {
    let mut specs = vec![
        preview(
            "preview_add_parameter_key",
            "预览添加参数关键点",
            "AddParameterKey",
            "预览给对象添加参数关键点。",
            false,
            vec![
                string("objectId", "ObjectId", true),
                string("parameterId", "ParameterId", true),
                number("keyValue", "KeyValue", true, None, None),
            ],
        ),
        preview(
            "preview_delete_parameter_key",
            "预览删除参数关键点",
            "DeleteParameterKey",
            "预览删除参数关键点。",
            true,
            vec![
                string("objectId", "ObjectId", false),
                string("parameterId", "ParameterId", false),
                boolean("strict", "Strict", false),
                number("keyValue", "KeyValue", false, None, None),
            ],
        ),
        preview(
            "preview_move_parameter_key",
            "预览移动参数关键点",
            "MoveParameterKey",
            "预览移动参数关键点。",
            true,
            vec![
                string("objectId", "ObjectId", false),
                string("parameterId", "ParameterId", false),
                number("fromValue", "FromValue", true, None, None),
                number("toValue", "ToValue", true, None, None),
                boolean("strict", "Strict", false),
                boolean("forceOverwrite", "ForceOverwrite", false),
            ],
        ),
        preview(
            "preview_add_parameter",
            "预览添加参数",
            "AddParameter",
            "预览添加参数。仅用于新建参数；若参数已存在并需迁入参数组，必须改用 preview_move_parameter。",
            false,
            vec![
                string("name", "Name", false),
                string("id", "Id", false),
                string("groupId", "GroupId", false),
                number("min", "Min", false, None, None),
                number("default", "Default", false, None, None),
                number("max", "Max", false, None, None),
                boolean("isBlendShape", "IsBlendShape", false),
            ],
        ),
        preview(
            "preview_add_parameter_group",
            "预览添加参数组",
            "AddParameterGroup",
            "预览添加参数组。",
            false,
            vec![string("name", "Name", false), string("id", "Id", false)],
        ),
        preview(
            "preview_edit_parameter",
            "预览编辑参数",
            "EditParameter",
            "预览编辑参数定义。",
            false,
            vec![
                string("id", "Id", true),
                string("newId", "NewId", false),
                string("name", "Name", false),
                number("min", "Min", false, None, None),
                number("default", "Default", false, None, None),
                number("max", "Max", false, None, None),
                boolean("isRepeat", "IsRepeat", false),
            ],
        ),
        preview(
            "preview_edit_parameter_group",
            "预览编辑参数组",
            "EditParameterGroup",
            "预览编辑参数组。",
            false,
            vec![
                string("id", "Id", true),
                string("newId", "NewId", false),
                string("name", "Name", false),
                choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
                color("labelCustomColor", "LabelCustomColor", false),
            ],
        ),
        preview(
            "preview_delete_parameter",
            "预览删除参数",
            "DeleteParameter",
            "预览删除参数。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_delete_parameter_group",
            "预览删除参数组",
            "DeleteParameterGroup",
            "预览删除参数组。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_move_parameter",
            "预览移动参数",
            "MoveParameter",
            "预览移动参数到参数组及指定位置。用于把已存在的参数迁入或迁出参数组。",
            true,
            vec![
                string("id", "Id", true),
                string("groupId", "GroupId", true),
                integer("insertIndex", "InsertIndex", false, 0, i32::MAX as i64),
            ],
        ),
        preview(
            "preview_move_parameter_group",
            "预览移动参数组",
            "MoveParameterGroup",
            "预览移动参数组顺序。",
            true,
            vec![
                string("id", "Id", true),
                integer("insertIndex", "InsertIndex", true, 0, i32::MAX as i64),
            ],
        ),
        preview(
            "preview_add_selected_objects",
            "预览添加对象选择",
            "AddSelectedObjects",
            "预览把对象加入当前选择。",
            false,
            vec![strings("ids", "Ids", false)],
        ),
        preview(
            "preview_clear_selected_objects",
            "预览清空对象选择",
            "ClearSelectedObjects",
            "预览清空当前对象选择。",
            false,
            vec![],
        ),
        preview(
            "preview_delete_object",
            "预览删除对象",
            "DeleteObject",
            "预览从 Part 面板删除对象。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_move_object_on_parts_palette",
            "预览移动 Part 对象",
            "MoveObjectOnPartsPalette",
            "预览移动 Part 面板中的对象。",
            true,
            vec![
                string("id", "Id", true),
                string("parentId", "ParentId", false),
                string("insertId", "InsertId", false),
                integer("insertIndex", "InsertIndex", false, 0, i32::MAX as i64),
            ],
        ),
        preview(
            "preview_add_part",
            "预览添加 Part",
            "AddPart",
            "预览添加 Part。",
            false,
            vec![
                string("name", "Name", false),
                string("id", "Id", false),
                number("drawOrder", "DrawOrder", false, Some(0.0), Some(1000.0)),
                strings("ids", "Ids", false),
                boolean("isNested", "IsNested", false),
            ],
        ),
    ];
    let mut edit_part = common_object_edit_fields("isExactMatch");
    edit_part.extend([
        boolean("isGrouped", "IsGrouped", false),
        boolean("isGuidImage", "IsGuidImage", false),
        boolean("isOffscreen", "IsOffscreen", false),
        strings("clippingIds", "ClippingIds", false),
        boolean("isReverseMask", "IsReverseMask", false),
        number("drawOrder", "DrawOrder", false, Some(0.0), Some(1000.0)),
        number("opacity", "Opacity", false, Some(0.0), Some(100.0)),
        color("multiplyColor", "MultiplyColor", false),
        color("screenColor", "ScreenColor", false),
        choice("colorBlend", "ColorBlend", false, COLOR_BLENDS),
        choice("alphaBlend", "AlphaBlend", false, ALPHA_BLENDS),
        choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
        color("labelCustomColor", "LabelCustomColor", false),
    ]);
    specs.push(preview(
        "preview_edit_part",
        "预览编辑 Part",
        "EditPart",
        "预览编辑 Part 属性。",
        false,
        edit_part,
    ));

    let mut edit_art_mesh = common_object_edit_fields("IsExactMatch");
    edit_art_mesh.extend([
        string("parentDeformerId", "ParentDeformerId", false),
        strings("clippingIds", "ClippingIds", false),
        boolean("isReverseMask", "IsReverseMask", false),
        number("drawOrder", "DrawOrder", false, Some(0.0), Some(1000.0)),
        number("opacity", "Opacity", false, Some(0.0), Some(100.0)),
        color("multiplyColor", "MultiplyColor", false),
        color("screenColor", "ScreenColor", false),
        choice("colorBlend", "ColorBlend", false, COLOR_BLENDS),
        choice("alphaBlend", "AlphaBlend", false, ALPHA_BLENDS),
        boolean("isCulling", "IsCulling", false),
        choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
        color("labelCustomColor", "LabelCustomColor", false),
    ]);
    specs.push(preview(
        "preview_edit_art_mesh",
        "预览编辑 ArtMesh",
        "EditArtMesh",
        "预览编辑 ArtMesh 的已公开属性；不编辑网格几何。",
        false,
        edit_art_mesh,
    ));

    let mut edit_glue = common_object_edit_fields("IsExactMatch");
    edit_glue.extend([
        number("intensity", "Intensity", false, Some(0.0), Some(100.0)),
        choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
        color("labelCustomColor", "LabelCustomColor", false),
    ]);
    specs.push(preview(
        "preview_edit_glue",
        "预览编辑 Glue",
        "EditGlue",
        "预览编辑已有 Glue 属性；不创建 Glue。",
        false,
        edit_glue,
    ));

    let add_deformer = vec![
        string("name", "Name", false),
        string("id", "Id", false),
        string("parentId", "ParentId", false),
        strings("targetObjectIds", "TargetObjectIds", false),
        choice("mode", "Mode", false, DEFORMER_MODES),
    ];
    specs.push(preview(
        "preview_add_rotation_deformer",
        "预览添加 Rotation Deformer",
        "AddRotationDeformer",
        "预览添加 Rotation Deformer。",
        false,
        add_deformer.clone(),
    ));
    let mut add_warp = add_deformer;
    add_warp.extend([
        integer("warpDivH", "WarpDivH", false, 2, 100),
        integer("warpDivV", "WarpDivV", false, 2, 100),
        integer("bezierDivH", "BezierDivH", false, 1, 100),
        integer("bezierDivV", "BezierDivV", false, 1, 100),
        boolean("considerChildKeyforms", "ConsiderChildKeyforms", false),
        boolean("snapCenter", "SnapCenter", false),
    ]);
    specs.push(preview(
        "preview_add_warp_deformer",
        "预览添加 Warp Deformer",
        "AddWarpDeformer",
        "预览添加 Warp Deformer。",
        false,
        add_warp,
    ));

    let mut edit_rotation = common_object_edit_fields("isExactMatch");
    edit_rotation.extend([
        string("parentDeformerId", "ParentDeformerId", false),
        number("angle", "Angle", false, None, None),
        number("baseAngle", "BaseAngle", false, None, None),
        number("scale", "Scale", false, None, None),
        number("opacity", "Opacity", false, Some(0.0), Some(100.0)),
        color("multiplyColor", "MultiplyColor", false),
        color("screenColor", "ScreenColor", false),
        choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
        color("labelCustomColor", "LabelCustomColor", false),
    ]);
    specs.push(preview(
        "preview_edit_rotation_deformer",
        "预览编辑 Rotation Deformer",
        "EditRotationDeformer",
        "预览编辑 Rotation Deformer。",
        false,
        edit_rotation,
    ));

    let mut edit_warp = common_object_edit_fields("isExactMatch");
    edit_warp.extend([
        string("parentDeformerId", "ParentDeformerId", false),
        number("opacity", "Opacity", false, Some(0.0), Some(100.0)),
        color("multiplyColor", "MultiplyColor", false),
        color("screenColor", "ScreenColor", false),
        integer("warpDivH", "WarpDivH", false, 2, 100),
        integer("warpDivV", "WarpDivV", false, 2, 100),
        integer("bezierDivH", "BezierDivH", false, 1, 100),
        integer("bezierDivV", "BezierDivV", false, 1, 100),
        choice("labelColorType", "LabelColorType", false, LABEL_COLORS),
        color("labelCustomColor", "LabelCustomColor", false),
    ]);
    specs.push(preview(
        "preview_edit_warp_deformer",
        "预览编辑 Warp Deformer",
        "EditWarpDeformer",
        "预览编辑 Warp Deformer。",
        false,
        edit_warp,
    ));
    specs
}

fn validate_constraints(method: &str, data: &Value) -> Result<(), CommandError> {
    if matches!(method, "DeleteParameterKey" | "MoveParameterKey")
        && data.get("ObjectId").is_none()
        && data.get("ParameterId").is_none()
        && data.get("Strict").and_then(Value::as_bool) != Some(false)
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "未指定 objectId 和 parameterId 时，strict 必须明确为 false。",
        ));
    }
    if method == "AddWarpDeformer"
        && data.get("SnapCenter").and_then(Value::as_bool) == Some(true)
        && data.get("ConsiderChildKeyforms").and_then(Value::as_bool) != Some(true)
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "snapCenter=true 仅在 considerChildKeyforms=true 时有效。",
        ));
    }
    if matches!(method, "AddRotationDeformer" | "AddWarpDeformer")
        && data.get("Mode").and_then(Value::as_str) == Some("AsChild")
        && data
            .get("TargetObjectIds")
            .and_then(Value::as_array)
            .map(Vec::len)
            != Some(1)
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "mode=AsChild 时 targetObjectIds 必须且只能包含一个对象。",
        ));
    }
    Ok(())
}

fn required_batch_id<'a>(
    data: &'a Value,
    key: &str,
    index: usize,
) -> Result<&'a str, CommandError> {
    data.get(key).and_then(Value::as_str).ok_or_else(|| {
        CommandError::new(
            "invalid_arguments",
            format!("第 {} 项缺少可稳定验证的 {key}。", index + 1),
        )
    })
}

fn key_scope(method: &str, data: &Value) -> Option<(String, String, String)> {
    let object_id = data.get("ObjectId")?.as_str()?.into();
    let parameter_id = data.get("ParameterId")?.as_str()?.into();
    let key = data
        .get(if method == "MoveParameterKey" {
            "FromValue"
        } else {
            "KeyValue"
        })
        .and_then(Value::as_f64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "*".into());
    Some((object_id, parameter_id, key))
}

fn batch_target(method: &str, data: &Value, index: usize) -> Result<Option<String>, CommandError> {
    let target = match method {
        "ClearSelectedObjects" => Some("selection".into()),
        "AddParameterKey" => Some(format!(
            "{}\u{0}{}\u{0}{}",
            required_batch_id(data, "ObjectId", index)?,
            required_batch_id(data, "ParameterId", index)?,
            data.get("KeyValue")
                .and_then(Value::as_f64)
                .unwrap_or_default()
        )),
        "DeleteParameterKey" | "MoveParameterKey" => {
            key_scope(method, data).map(|(object_id, parameter_id, key)| {
                format!("{object_id}\u{0}{parameter_id}\u{0}{key}")
            })
        }
        "AddParameter"
        | "AddParameterGroup"
        | "AddPart"
        | "AddRotationDeformer"
        | "AddWarpDeformer" => data.get("Id").and_then(Value::as_str).map(str::to_string),
        "EditParameter"
        | "EditParameterGroup"
        | "DeleteParameter"
        | "DeleteParameterGroup"
        | "MoveParameter"
        | "MoveParameterGroup"
        | "DeleteObject"
        | "MoveObjectOnPartsPalette"
        | "EditPart"
        | "EditArtMesh"
        | "EditGlue"
        | "EditRotationDeformer"
        | "EditWarpDeformer" => Some(required_batch_id(data, "Id", index)?.into()),
        "AddSelectedObjects" => None,
        _ => None,
    };
    Ok(target)
}

pub(super) fn validate_batch_conflicts(
    method: &str,
    operations: &[Value],
) -> Result<(), CommandError> {
    let mut targets = BTreeMap::<String, usize>::new();
    let mut final_ids = BTreeMap::<String, usize>::new();
    let mut source_ids = BTreeMap::<String, usize>::new();
    let mut affected_ids = BTreeMap::<String, usize>::new();
    let mut key_scopes = BTreeMap::<(String, String), Vec<(String, usize)>>::new();
    let mut key_destinations = BTreeMap::<(String, String, String), usize>::new();
    let mut ordered_destinations = BTreeMap::<String, usize>::new();

    for (index, data) in operations.iter().enumerate() {
        if let Some(target) = batch_target(method, data, index)? {
            if let Some(previous) = targets.insert(target.clone(), index) {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!(
                        "第 {} 项与第 {} 项修改同一目标，无法形成可验证批次。",
                        previous + 1,
                        index + 1
                    ),
                ));
            }
        }
        if matches!(method, "DeleteParameterKey" | "MoveParameterKey")
            && operations.len() > 1
            && (data.get("ObjectId").is_none() || data.get("ParameterId").is_none())
        {
            return Err(CommandError::new(
                "invalid_arguments",
                format!(
                    "第 {} 项的关键点范围过宽，无法确认与批次内其他项互不重叠。",
                    index + 1
                ),
            ));
        }
        if matches!(method, "DeleteParameterKey" | "MoveParameterKey") {
            let (object_id, parameter_id, key) = key_scope(method, data)
                .unwrap_or_else(|| (String::new(), String::new(), "*".into()));
            let entries = key_scopes
                .entry((object_id.clone(), parameter_id.clone()))
                .or_default();
            if let Some((_, previous)) = entries
                .iter()
                .find(|(existing, _)| existing == "*" || key == "*" || existing == &key)
            {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!(
                        "第 {} 项与第 {} 项的关键点范围重叠。",
                        previous + 1,
                        index + 1
                    ),
                ));
            }
            entries.push((key, index));
            if method == "MoveParameterKey" {
                let to = data
                    .get("ToValue")
                    .and_then(Value::as_f64)
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                if let Some(previous) =
                    key_destinations.insert((object_id, parameter_id, to), index)
                {
                    return Err(CommandError::new(
                        "invalid_arguments",
                        format!(
                            "第 {} 项与第 {} 项会生成同一参数关键点。",
                            previous + 1,
                            index + 1
                        ),
                    ));
                }
            }
        }

        if let Some(source) = data.get("Id").and_then(Value::as_str) {
            source_ids.insert(source.into(), index);
        }
        if method == "MoveObjectOnPartsPalette"
            && (data.get("InsertIndex").is_some() || data.get("InsertId").is_some())
        {
            let destination = data
                .get("ParentId")
                .and_then(Value::as_str)
                .unwrap_or("<root>")
                .to_string();
            if let Some(previous) = ordered_destinations.insert(destination, index) {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!(
                        "第 {} 项与第 {} 项会共同改变同一 Part 容器的顺序，当前无法可靠验证。",
                        previous + 1,
                        index + 1
                    ),
                ));
            }
        }
        let final_id = if method.starts_with("Add") && method != "AddSelectedObjects" {
            data.get("Id").and_then(Value::as_str)
        } else {
            data.get("NewId").and_then(Value::as_str)
        };
        if let Some(final_id) = final_id {
            if let Some(previous) = final_ids.insert(final_id.into(), index) {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!(
                        "第 {} 项与第 {} 项会产生相同 ID {final_id}。",
                        previous + 1,
                        index + 1
                    ),
                ));
            }
        }

        let affected = match method {
            "AddSelectedObjects" => data.get("Ids"),
            "AddPart" => data.get("Ids"),
            "AddRotationDeformer" | "AddWarpDeformer" => data.get("TargetObjectIds"),
            _ => None,
        };
        if let Some(ids) = affected.and_then(Value::as_array) {
            for id in ids.iter().filter_map(Value::as_str) {
                if let Some(previous) = affected_ids.insert(id.into(), index) {
                    return Err(CommandError::new(
                        "invalid_arguments",
                        format!(
                            "第 {} 项与第 {} 项重复引用对象 {id}。",
                            previous + 1,
                            index + 1
                        ),
                    ));
                }
            }
        }
    }

    for (id, final_index) in final_ids {
        if let Some(source_index) = source_ids.get(&id).filter(|index| **index != final_index) {
            return Err(CommandError::new(
                "invalid_arguments",
                format!(
                    "第 {} 项产生的 ID {id} 与第 {} 项的源 ID 存在顺序依赖。",
                    final_index + 1,
                    source_index + 1
                ),
            ));
        }
    }
    for (index, data) in operations.iter().enumerate() {
        for key in ["ParentId", "ParentDeformerId"] {
            if let Some(parent_id) = data.get(key).and_then(Value::as_str) {
                if data.get("Id").and_then(Value::as_str) == Some(parent_id) {
                    return Err(CommandError::new(
                        "invalid_arguments",
                        format!("第 {} 项不能把对象设为自身的父级。", index + 1),
                    ));
                }
                if let Some(parent_index) =
                    source_ids.get(parent_id).filter(|other| **other != index)
                {
                    return Err(CommandError::new(
                        "invalid_arguments",
                        format!(
                            "第 {} 项把目标挂到第 {} 项正在修改的对象 {parent_id}，无法安全判定批量层级。",
                            index + 1,
                            parent_index + 1
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) async fn preview_edit(
    service: &EditorService,
    spec: &ToolSpec,
    args: Value,
) -> Result<Value, CommandError> {
    let (rpc, model_uid, generation) = session(service, true).await?;
    let model_uid =
        model_uid.ok_or_else(|| CommandError::new("missing_model", "当前没有可编辑模型。"))?;
    let (public_operations, data): (Vec<_>, Vec<_>) = normalize_operations(spec, args, &model_uid)?
        .into_iter()
        .unzip();
    for operation in &data {
        validate_constraints(spec.method, operation)?;
    }
    validate_batch_conflicts(spec.method, &data)?;

    let mut items = Vec::with_capacity(data.len());
    for (index, data) in data.into_iter().enumerate() {
        let snapshot = verification_snapshot(&rpc, spec.method, &data)
            .await
            .map_err(CommandError::from)?;
        let precondition = edit_precondition(spec.method, &data, &snapshot).map_err(|error| {
            CommandError::new(
                if error.invalid_target {
                    "invalid_arguments"
                } else {
                    "protocol_error"
                },
                format!("第 {} 项：{}", index + 1, error.message),
            )
        })?;
        items.push(StoredEditorEditItem { data, precondition });
    }
    let preview_id = Uuid::new_v4().simple().to_string();
    let summary = format!("{} 共 {} 项", spec.method, public_operations.len());
    let plan = StoredEditorEditPlan {
        preview_id: preview_id.clone(),
        generation,
        model_uid,
        method: spec.method.into(),
        items,
    };
    if matches!(spec.method, "MoveParameter" | "MoveParameterGroup")
        && expected_ordered_move_positions(&plan).is_none()
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "批量移动的目标顺序无法从当前模型稳定计算。",
        ));
    }
    {
        let mut inner = service.inner.lock().await;
        if inner.generation != generation || inner.operation.is_some() {
            return Err(CommandError::new(
                "stale_preview",
                "连接或模型在预览期间发生变化，请重试。",
            ));
        }
        inner.editor_edit_previews.insert(preview_id.clone(), plan);
    }
    serde_json::to_value(EditorEditPreview {
        preview_id,
        operation: spec.method.into(),
        summary,
        operation_count: public_operations.len(),
        operations: public_operations,
        destructive: spec.destructive,
    })
    .map_err(|error| CommandError::new("serialization_error", error.to_string()))
}
