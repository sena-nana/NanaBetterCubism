use super::{
    read::session,
    schema::{
        boolean, choice, color, common_object_edit_fields, integer, normalize_arguments, number,
        preview, string, strings, ToolSpec, ALPHA_BLENDS, COLOR_BLENDS, DEFORMER_MODES,
        LABEL_COLORS,
    },
    verification::verification_snapshot,
    CommandError, EditorService,
};
use crate::domain::{EditorEditPreview, StoredEditorEditPlan};
use serde_json::Value;
use uuid::Uuid;

pub(super) fn specs() -> Vec<ToolSpec> {
    let mut specs = vec![
        preview(
            "preview_add_parameter_key",
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
            "AddParameter",
            "预览添加参数。",
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
            "AddParameterGroup",
            "预览添加参数组。",
            false,
            vec![string("name", "Name", false), string("id", "Id", false)],
        ),
        preview(
            "preview_edit_parameter",
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
            "DeleteParameter",
            "预览删除参数。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_delete_parameter_group",
            "DeleteParameterGroup",
            "预览删除参数组。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_move_parameter",
            "MoveParameter",
            "预览移动参数到参数组及指定位置。",
            true,
            vec![
                string("id", "Id", true),
                string("groupId", "GroupId", true),
                integer("insertIndex", "InsertIndex", false, 0, i32::MAX as i64),
            ],
        ),
        preview(
            "preview_move_parameter_group",
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
            "AddSelectedObjects",
            "预览把对象加入当前选择。",
            false,
            vec![strings("ids", "Ids", false)],
        ),
        preview(
            "preview_clear_selected_objects",
            "ClearSelectedObjects",
            "预览清空当前对象选择。",
            false,
            vec![],
        ),
        preview(
            "preview_delete_object",
            "DeleteObject",
            "预览从 Part 面板删除对象。",
            true,
            vec![string("id", "Id", true)],
        ),
        preview(
            "preview_move_object_on_parts_palette",
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

pub(super) async fn preview_edit(
    service: &EditorService,
    spec: &ToolSpec,
    args: Value,
) -> Result<Value, CommandError> {
    let (rpc, model_uid, generation) = session(service, true).await?;
    let model_uid =
        model_uid.ok_or_else(|| CommandError::new("missing_model", "当前没有可编辑模型。"))?;
    let data = normalize_arguments(spec, args.clone(), Some(&model_uid))?;
    validate_constraints(spec.method, &data)?;

    let precondition = verification_snapshot(&rpc, spec.method, &data)
        .await
        .map_err(CommandError::from)?;
    let preview_id = Uuid::new_v4().simple().to_string();
    let summary = format!(
        "{} {}",
        spec.method,
        serde_json::to_string(&args).unwrap_or_default()
    );
    let plan = StoredEditorEditPlan {
        preview_id: preview_id.clone(),
        generation,
        model_uid,
        method: spec.method.into(),
        data,
        precondition,
    };
    {
        let mut inner = service.inner.lock().await;
        if inner.generation != generation || inner.operation.is_some() {
            return Err(CommandError::new(
                "stale_preview",
                "连接或模型在预览期间发生变化，请重试。",
            ));
        }
        inner.editor_edit_previews.clear();
        inner.editor_edit_previews.insert(preview_id.clone(), plan);
    }
    serde_json::to_value(EditorEditPreview {
        preview_id,
        operation: spec.method.into(),
        summary,
        arguments: args,
        destructive: spec.destructive,
    })
    .map_err(|error| CommandError::new("serialization_error", error.to_string()))
}
