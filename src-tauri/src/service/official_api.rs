use super::{
    transaction::{mutation_request, require_execution_true, require_true, ExecutionError},
    ActiveOperation, CommandError, EditorService,
};
use crate::domain::{
    EditorConnectionState, EditorEditOutcome, EditorEditPreview, EditorEditResult,
    OperationAccepted, StoredEditorEditPlan,
};
use crate::protocol::{RpcClient, RpcError};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolMode {
    Direct,
    Preview,
}

#[derive(Clone, Copy)]
enum FieldKind {
    String { max_len: Option<usize> },
    Number { min: Option<f64>, max: Option<f64> },
    Integer { min: i64, max: i64 },
    Boolean,
    StringArray,
    ParameterValues,
    ParameterFilters,
    Color,
    Choice(&'static [&'static str]),
}

#[derive(Clone, Copy)]
struct FieldSpec {
    input: &'static str,
    editor: &'static str,
    kind: FieldKind,
    required: bool,
}

#[derive(Clone)]
struct ToolSpec {
    tool_name: &'static str,
    method: &'static str,
    description: &'static str,
    mode: ToolMode,
    fields: Vec<FieldSpec>,
    uses_model: bool,
    destructive: bool,
}

const LABEL_COLORS: &[&str] = &[
    "Undefined",
    "Red",
    "Orange",
    "Yellow",
    "Green",
    "Blue",
    "Purple",
    "Gray",
    "Custom",
];
const COLOR_BLENDS: &[&str] = &[
    "Normal",
    "Add",
    "AddGlow",
    "Darken",
    "Multiply",
    "ColorBurn",
    "LinearBurn",
    "Lighten",
    "Screen",
    "ColorDodge",
    "Overlay",
    "SoftLight",
    "HardLight",
    "LinearLight",
    "Hue",
    "Color",
    "Add_5.2",
    "Multiply_5.2",
];
const ALPHA_BLENDS: &[&str] = &["Over", "Atop", "Out", "Conjoint", "Disjoint"];
const DEFORMER_MODES: &[&str] = &["AsParent", "AsChild"];
const LOG_TYPES: &[&str] = &["info", "warning"];

fn string(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::String { max_len: None },
        required,
    }
}

fn limited_string(
    input: &'static str,
    editor: &'static str,
    required: bool,
    max_len: usize,
) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::String {
            max_len: Some(max_len),
        },
        required,
    }
}

fn number(
    input: &'static str,
    editor: &'static str,
    required: bool,
    min: Option<f64>,
    max: Option<f64>,
) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::Number { min, max },
        required,
    }
}

fn integer(
    input: &'static str,
    editor: &'static str,
    required: bool,
    min: i64,
    max: i64,
) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::Integer { min, max },
        required,
    }
}

fn boolean(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::Boolean,
        required,
    }
}

fn strings(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::StringArray,
        required,
    }
}

fn parameter_values(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::ParameterValues,
        required,
    }
}

fn parameter_filters(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::ParameterFilters,
        required,
    }
}

fn color(input: &'static str, editor: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::Color,
        required,
    }
}

fn choice(
    input: &'static str,
    editor: &'static str,
    required: bool,
    choices: &'static [&'static str],
) -> FieldSpec {
    FieldSpec {
        input,
        editor,
        kind: FieldKind::Choice(choices),
        required,
    }
}

fn direct(
    tool_name: &'static str,
    method: &'static str,
    description: &'static str,
    uses_model: bool,
    fields: Vec<FieldSpec>,
) -> ToolSpec {
    ToolSpec {
        tool_name,
        method,
        description,
        mode: ToolMode::Direct,
        fields,
        uses_model,
        destructive: false,
    }
}

fn preview(
    tool_name: &'static str,
    method: &'static str,
    description: &'static str,
    destructive: bool,
    fields: Vec<FieldSpec>,
) -> ToolSpec {
    ToolSpec {
        tool_name,
        method,
        description,
        mode: ToolMode::Preview,
        fields,
        uses_model: true,
        destructive,
    }
}

fn common_object_edit_fields(is_exact_match_key: &'static str) -> Vec<FieldSpec> {
    vec![
        string("id", "Id", true),
        parameter_filters("parameters", "Parameters", false),
        boolean("isExactMatch", is_exact_match_key, false),
        string("newId", "NewId", false),
        string("name", "Name", false),
        string("parentId", "ParentId", false),
    ]
}

fn tool_specs() -> Vec<ToolSpec> {
    let mut specs = vec![
        direct(
            "get_parameter_values",
            "GetParameterValues",
            "读取当前模型的参数值；不包含动画信息。",
            true,
            vec![strings("ids", "Ids", false)],
        ),
        direct(
            "set_parameter_values",
            "SetParameterValues",
            "向当前模型的 Editor 临时参数缓冲区写入参数值。",
            true,
            vec![parameter_values("parameters", "Parameters", true)],
        ),
        direct(
            "get_parameters",
            "GetParameters",
            "读取当前模型参数及关键点定义。",
            true,
            vec![],
        ),
        direct(
            "get_parameter_groups",
            "GetParameterGroups",
            "读取当前模型的参数组。",
            true,
            vec![],
        ),
        direct(
            "list_editor_documents",
            "GetDocuments",
            "列出 Editor 当前打开的建模、物理和动画文档。",
            false,
            vec![],
        ),
        direct(
            "get_editor_document",
            "GetDocument",
            "按 list_editor_documents 返回的 documentRef 读取文档。",
            false,
            vec![string("documentRef", "", true)],
        ),
        direct(
            "get_current_document",
            "GetCurrentDocumentUID",
            "读取 Editor 当前文档信息。",
            false,
            vec![],
        ),
        direct(
            "get_current_model",
            "GetCurrentModelUID",
            "确认 Editor 当前是否有模型。",
            false,
            vec![],
        ),
        direct(
            "get_current_edit_mode",
            "GetCurrentEditMode",
            "读取 Editor 当前编辑模式。",
            false,
            vec![],
        ),
        direct(
            "clear_parameter_values",
            "ClearParameterValues",
            "清除当前模型由外部应用写入的临时参数值。",
            true,
            vec![],
        ),
        direct(
            "get_physics_info",
            "GetPhysicsInfo",
            "访问物理设置编辑器的计算 FPS 接口。",
            true,
            vec![number("fps", "Fps", false, Some(0.0), None)],
        ),
        direct(
            "send_cubism_log",
            "SendCubismLog",
            "向 Cubism Editor 日志面板发送一条日志。",
            false,
            vec![
                choice("type", "Type", false, LOG_TYPES),
                limited_string("message", "Message", true, 5000),
                boolean("display", "Display", false),
            ],
        ),
        direct(
            "notify_physics_file_exported",
            "NotifyPhysicsFileExported",
            "启用或停用物理设置文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        direct(
            "notify_moc_file_exported",
            "NotifyMocFileExported",
            "启用或停用 MOC3 及相关文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        direct(
            "notify_motion_file_exported",
            "NotifyMotionFileExported",
            "启用或停用 motion 文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        direct(
            "notify_motion_sync_file_exported",
            "NotifyMotionSyncFileExported",
            "启用或停用 motion-sync 文件导出通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        direct(
            "notify_change_edit_mode",
            "NotifyChangeEditMode",
            "启用或停用 Editor 模式切换通知。",
            false,
            vec![boolean("enabled", "Enabled", true)],
        ),
        direct(
            "get_parameter_keys",
            "GetParameterKeys",
            "读取对象关联参数的关键点值。",
            true,
            vec![string("objectId", "ObjectId", true)],
        ),
        direct(
            "get_objects_by_parameter_key",
            "GetObjectsByParameterKeys",
            "读取与指定参数关键点关联的对象。",
            true,
            vec![
                string("parameterId", "ParameterId", true),
                number("keyValue", "KeyValue", true, None, None),
            ],
        ),
        direct(
            "get_parameter_structure",
            "GetParameterStructure",
            "读取当前模型的完整参数与参数组结构。",
            true,
            vec![],
        ),
        direct(
            "get_selected_objects",
            "GetSelectedObjecs",
            "读取当前模型已选择的对象 ID。",
            true,
            vec![],
        ),
        direct(
            "get_part_structure",
            "GetPartStructure",
            "读取当前模型的 Part 与对象层级。",
            true,
            vec![],
        ),
        direct(
            "get_object",
            "GetObject",
            "读取 Part、ArtMesh、Glue 或 Deformer 属性。",
            true,
            vec![
                string("id", "Id", true),
                parameter_filters("parameters", "Parameters", false),
            ],
        ),
        direct(
            "get_deformer_structure",
            "GetDeformerStructure",
            "读取当前模型的 Deformer 层级。",
            true,
            vec![],
        ),
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

fn field_schema(field: &FieldSpec) -> Value {
    match field.kind {
        FieldKind::String { max_len } => {
            let mut schema = json!({"type": "string", "minLength": 1});
            if let Some(max_len) = max_len {
                schema["maxLength"] = json!(max_len);
            }
            schema
        }
        FieldKind::Number { min, max } => {
            let mut schema = json!({"type": "number"});
            if let Some(min) = min {
                schema["minimum"] = json!(min);
            }
            if let Some(max) = max {
                schema["maximum"] = json!(max);
            }
            schema
        }
        FieldKind::Integer { min, max } => {
            json!({"type": "integer", "minimum": min, "maximum": max})
        }
        FieldKind::Boolean => json!({"type": "boolean"}),
        FieldKind::StringArray => {
            json!({"type": "array", "items": {"type": "string", "minLength": 1}})
        }
        FieldKind::ParameterValues => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "minLength": 1},
                    "value": {"type": "number"}
                },
                "required": ["id", "value"],
                "additionalProperties": false
            }
        }),
        FieldKind::ParameterFilters => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "minLength": 1},
                    "value": {"type": "number"}
                },
                "anyOf": [{"required": ["id"]}, {"required": ["value"]}],
                "additionalProperties": false
            }
        }),
        FieldKind::Color => json!({"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"}),
        FieldKind::Choice(values) => json!({"type": "string", "enum": values}),
    }
}

fn function_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

pub(crate) fn tool_definitions() -> Vec<Value> {
    let mut tools = tool_specs()
        .into_iter()
        .map(|spec| {
            let properties = spec
                .fields
                .iter()
                .map(|field| (field.input.to_string(), field_schema(field)))
                .collect::<Map<_, _>>();
            let required = spec
                .fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.input)
                .collect::<Vec<_>>();
            function_tool(
                spec.tool_name,
                spec.description,
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false
                }),
            )
        })
        .collect::<Vec<_>>();
    tools.extend([
        function_tool(
            "list_editor_notifications",
            "读取当前 Editor 连接已收到的官方事件通知。",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        function_tool(
            "execute_editor_edit",
            "执行已确认的官方编辑 API 预览；返回 operationId。",
            json!({
                "type": "object",
                "properties": {"previewId": {"type": "string", "minLength": 1}},
                "required": ["previewId"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "get_editor_edit_result",
            "查询官方编辑 API 操作的真实事务与回读结果。",
            json!({
                "type": "object",
                "properties": {"operationId": {"type": "string", "minLength": 1}},
                "required": ["operationId"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "cancel_editor_edit",
            "请求取消正在执行的官方编辑 API 事务。",
            json!({
                "type": "object",
                "properties": {"operationId": {"type": "string", "minLength": 1}},
                "required": ["operationId"],
                "additionalProperties": false
            }),
        ),
    ]);
    tools
}

pub(crate) fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "execute_editor_edit"
            | "get_editor_edit_result"
            | "cancel_editor_edit"
            | "list_editor_notifications"
    ) || tool_specs().iter().any(|spec| spec.tool_name == name)
}

fn spec(name: &str) -> Option<ToolSpec> {
    tool_specs().into_iter().find(|spec| spec.tool_name == name)
}

fn validate_value(field: &FieldSpec, value: &Value) -> Result<Value, CommandError> {
    let invalid = || {
        CommandError::new(
            "invalid_arguments",
            format!("参数 {} 的类型或范围不符合官方 API 定义。", field.input),
        )
    };
    match field.kind {
        FieldKind::String { max_len } => {
            let value = value.as_str().ok_or_else(invalid)?;
            if value.is_empty() || max_len.is_some_and(|max| value.chars().count() > max) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Number { min, max } => {
            let value = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(invalid)?;
            if min.is_some_and(|min| value < min) || max.is_some_and(|max| value > max) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Integer { min, max } => {
            let value = value.as_i64().ok_or_else(invalid)?;
            if value < min || value > max {
                return Err(invalid());
            }
            Ok(json!(value))
        }
        FieldKind::Boolean => value.as_bool().map(Value::Bool).ok_or_else(invalid),
        FieldKind::StringArray => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for value in values {
                let value = value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(invalid)?;
                normalized.push(json!(value));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::ParameterValues => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for value in values {
                let object = value.as_object().ok_or_else(invalid)?;
                if object.keys().any(|key| key != "id" && key != "value") {
                    return Err(invalid());
                }
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(invalid)?;
                let number = object
                    .get("value")
                    .and_then(Value::as_f64)
                    .filter(|value| value.is_finite())
                    .ok_or_else(invalid)?;
                normalized.push(json!({"Id": id, "Value": number}));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::ParameterFilters => {
            let values = value.as_array().ok_or_else(invalid)?;
            let mut normalized = Vec::with_capacity(values.len());
            for value in values {
                let object = value.as_object().ok_or_else(invalid)?;
                if object.is_empty() || object.keys().any(|key| key != "id" && key != "value") {
                    return Err(invalid());
                }
                let mut item = Map::new();
                if let Some(id) = object.get("id") {
                    let id = id
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(invalid)?;
                    item.insert("Id".into(), json!(id));
                }
                if let Some(number) = object.get("value") {
                    let number = number
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .ok_or_else(invalid)?;
                    item.insert("Value".into(), json!(number));
                }
                normalized.push(Value::Object(item));
            }
            Ok(Value::Array(normalized))
        }
        FieldKind::Color => {
            let value = value.as_str().ok_or_else(invalid)?;
            let bytes = value.as_bytes();
            if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(invalid());
            }
            Ok(json!(value.to_ascii_uppercase()))
        }
        FieldKind::Choice(choices) => {
            let value = value.as_str().ok_or_else(invalid)?;
            if !choices.contains(&value) {
                return Err(invalid());
            }
            Ok(json!(value))
        }
    }
}

fn normalize_arguments(
    spec: &ToolSpec,
    args: Value,
    model_uid: Option<&str>,
) -> Result<Value, CommandError> {
    let args = args
        .as_object()
        .ok_or_else(|| CommandError::new("invalid_arguments", "工具参数必须是 JSON 对象。"))?;
    let allowed = spec
        .fields
        .iter()
        .map(|field| field.input)
        .collect::<BTreeSet<_>>();
    if let Some(unknown) = args.keys().find(|key| !allowed.contains(key.as_str())) {
        return Err(CommandError::new(
            "invalid_arguments",
            format!("未知参数 {unknown}。"),
        ));
    }
    let mut data = Map::new();
    if spec.uses_model {
        data.insert(
            "ModelUID".into(),
            json!(model_uid.ok_or_else(|| {
                CommandError::new("missing_model", "当前没有可用模型。")
            })?),
        );
    }
    for field in &spec.fields {
        match args.get(field.input) {
            Some(value) => {
                if !field.editor.is_empty() {
                    data.insert(field.editor.into(), validate_value(field, value)?);
                }
            }
            None if field.required => {
                return Err(CommandError::new(
                    "invalid_arguments",
                    format!("缺少参数 {}。", field.input),
                ));
            }
            None => {}
        }
    }
    Ok(Value::Object(data))
}

fn lower_first(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_lowercase().chain(chars).collect()
}

fn sanitize_response(value: Value) -> Value {
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

fn hash_value(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

async fn session(
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

async fn execute_direct(
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
        .map_err(CommandError::from)?;
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

async fn verification_snapshot(
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

fn verify_postcondition(plan: &StoredEditorEditPlan, snapshot: &Value) -> Option<bool> {
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

pub(crate) async fn call_tool(
    service: &EditorService,
    name: &str,
    args: Value,
) -> Result<Value, CommandError> {
    if name == "list_editor_notifications" {
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
        return Ok(json!({"notifications": notifications}));
    }
    let spec = spec(name)
        .ok_or_else(|| CommandError::new("unknown_tool", format!("未知 Editor 工具：{name}")))?;
    if spec.mode == ToolMode::Direct {
        return execute_direct(service, &spec, args).await;
    }
    let (rpc, model_uid, generation) = session(service, true).await?;
    let model_uid =
        model_uid.ok_or_else(|| CommandError::new("missing_model", "当前没有可编辑模型。"))?;
    let data = normalize_arguments(&spec, args.clone(), Some(&model_uid))?;
    if matches!(spec.method, "DeleteParameterKey" | "MoveParameterKey")
        && data.get("ObjectId").is_none()
        && data.get("ParameterId").is_none()
        && data.get("Strict").and_then(Value::as_bool) != Some(false)
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "未指定 objectId 和 parameterId 时，strict 必须明确为 false。",
        ));
    }
    if spec.method == "AddWarpDeformer"
        && data.get("SnapCenter").and_then(Value::as_bool) == Some(true)
        && data.get("ConsiderChildKeyforms").and_then(Value::as_bool) != Some(true)
    {
        return Err(CommandError::new(
            "invalid_arguments",
            "snapCenter=true 仅在 considerChildKeyforms=true 时有效。",
        ));
    }
    if matches!(spec.method, "AddRotationDeformer" | "AddWarpDeformer")
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

impl EditorService {
    pub(crate) async fn execute_editor_edit(
        &self,
        app: AppHandle,
        preview_id: String,
        cancel: Arc<AtomicBool>,
    ) -> Result<OperationAccepted, CommandError> {
        let (operation_id, plan, rpc) = {
            let mut inner = self.inner.lock().await;
            if inner.operation.is_some() {
                return Err(CommandError::new(
                    "operation_active",
                    "已有 Editor 编辑事务正在执行。",
                ));
            }
            let plan = inner
                .editor_edit_previews
                .remove(&preview_id)
                .ok_or_else(|| CommandError::new("stale_preview", "预览已失效，请重新预览。"))?;
            if plan.preview_id != preview_id
                || plan.generation != inner.generation
                || inner.model_uid.as_deref() != Some(&plan.model_uid)
            {
                return Err(CommandError::new(
                    "stale_preview",
                    "连接或模型已变化，请重新预览。",
                ));
            }
            let rpc = inner
                .rpc
                .clone()
                .ok_or_else(|| CommandError::new("disconnected", "Editor 连接不可用。"))?;
            let operation_id = Uuid::new_v4().simple().to_string();
            inner.operation = Some(ActiveOperation {
                id: operation_id.clone(),
                cancel: cancel.clone(),
            });
            inner.editor_edit_results.insert(
                operation_id.clone(),
                EditorEditResult {
                    operation_id: operation_id.clone(),
                    operation: plan.method.clone(),
                    outcome: EditorEditOutcome::Running,
                    message: "编辑事务正在执行。".into(),
                    verification: None,
                },
            );
            inner.snapshot.state = EditorConnectionState::Editing;
            inner.snapshot.capabilities.batch_create_parameters = false;
            inner.snapshot.capabilities.find_part_parameters = false;
            inner.snapshot.capabilities.official_edit_api = false;
            inner.snapshot.message = format!("正在执行 {}…", plan.method);
            (operation_id, plan, rpc)
        };
        self.emit_snapshot(&app).await;
        let service = self.clone();
        let accepted = OperationAccepted {
            operation_id: operation_id.clone(),
        };
        tokio::spawn(async move {
            service
                .run_editor_edit(app, operation_id, plan, rpc, cancel)
                .await;
        });
        Ok(accepted)
    }

    pub(crate) async fn editor_edit_result(
        &self,
        operation_id: &str,
    ) -> Result<EditorEditResult, CommandError> {
        self.inner
            .lock()
            .await
            .editor_edit_results
            .get(operation_id)
            .cloned()
            .ok_or_else(|| CommandError::new("missing_operation", "没有该编辑操作。"))
    }

    async fn run_editor_edit(
        &self,
        app: AppHandle,
        operation_id: String,
        plan: StoredEditorEditPlan,
        rpc: RpcClient,
        cancel: Arc<AtomicBool>,
    ) {
        let result = self.run_editor_edit_inner(&rpc, &plan, &cancel).await;
        self.finish_editor_edit(&app, &operation_id, result).await;
    }

    async fn run_editor_edit_inner(
        &self,
        rpc: &RpcClient,
        plan: &StoredEditorEditPlan,
        cancel: &AtomicBool,
    ) -> EditorEditResult {
        let operation_id = self
            .inner
            .lock()
            .await
            .operation
            .as_ref()
            .map(|operation| operation.id.clone())
            .unwrap_or_default();
        let result = |outcome, message: String, verification| EditorEditResult {
            operation_id: operation_id.clone(),
            operation: plan.method.clone(),
            outcome,
            message,
            verification,
        };
        let current_model = match rpc.request("GetCurrentModelUID", json!({})).await {
            Ok(value) => value,
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        };
        if current_model.get("ModelUID").and_then(Value::as_str) != Some(&plan.model_uid) {
            return result(
                EditorEditOutcome::Failed,
                "当前模型已变化，请重新预览。".into(),
                None,
            );
        }
        let precondition = match verification_snapshot(rpc, &plan.method, &plan.data).await {
            Ok(value) => value,
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        };
        if hash_value(&precondition) != hash_value(&plan.precondition) {
            return result(
                EditorEditOutcome::Failed,
                "目标模型状态已变化，请重新预览。".into(),
                None,
            );
        }
        let mut events = rpc.subscribe();
        match rpc
            .request("NotifyUndoCancel", json!({"Enabled": true}))
            .await
        {
            Ok(value) if value.get("Accepted").and_then(Value::as_bool) == Some(true) => {}
            Ok(_) => {
                return result(
                    EditorEditOutcome::Failed,
                    "Editor 未接受撤销取消通知，未开始编辑。".into(),
                    None,
                );
            }
            Err(error) => {
                return result(EditorEditOutcome::Failed, error.to_string(), None);
            }
        }
        let begin = mutation_request(
            rpc,
            &mut events,
            cancel,
            "EditBegin",
            json!({"Silent": false}),
        )
        .await
        .and_then(require_execution_true);
        if let Err(error) = begin {
            return match error {
                ExecutionError::UserCancelled(true) => result(
                    EditorEditOutcome::CancelledRolledBack,
                    "Editor 已在事务开始前取消操作。".into(),
                    None,
                ),
                ExecutionError::UserCancelled(false) => result(
                    EditorEditOutcome::Unknown,
                    "Editor 通知取消，但未确认恢复结果。".into(),
                    None,
                ),
                error => result(EditorEditOutcome::Failed, error.to_string(), None),
            };
        }
        let mut mutation = mutation_request(
            rpc,
            &mut events,
            cancel,
            "EditSendLog",
            json!({"Message": "正在执行已确认的模型编辑。"}),
        )
        .await
        .map(|_| ());
        if mutation.is_ok() {
            mutation = mutation_request(rpc, &mut events, cancel, &plan.method, plan.data.clone())
                .await
                .and_then(require_execution_true)
                .map(|_| ());
        }
        if mutation.is_ok() {
            mutation = mutation_request(
                rpc,
                &mut events,
                cancel,
                "EditSendProgress",
                json!({"Value": 1.0}),
            )
            .await
            .map(|_| ());
        }
        if let Err(error) = mutation {
            if let ExecutionError::UserCancelled(restored) = error {
                return result(
                    if restored {
                        EditorEditOutcome::CancelledRolledBack
                    } else {
                        EditorEditOutcome::Unknown
                    },
                    if restored {
                        "Editor 已取消并恢复编辑前状态。".into()
                    } else {
                        "Editor 通知取消，但未确认恢复结果。".into()
                    },
                    None,
                );
            }
            let rollback = rpc
                .request("EditEnd", json!({"Cancel": true}))
                .await
                .and_then(require_true);
            return result(
                if rollback.is_ok() {
                    EditorEditOutcome::FailedRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if rollback.is_ok() {
                    format!("编辑失败，Editor 已确认回滚：{error}")
                } else {
                    format!("编辑失败且无法确认回滚：{error}")
                },
                None,
            );
        }
        if cancel.load(Ordering::SeqCst) {
            let rollback = rpc
                .request("EditEnd", json!({"Cancel": true}))
                .await
                .and_then(require_true);
            return result(
                if rollback.is_ok() {
                    EditorEditOutcome::CancelledRolledBack
                } else {
                    EditorEditOutcome::Unknown
                },
                if rollback.is_ok() {
                    "已取消并恢复编辑前状态。".into()
                } else {
                    "已请求取消，但无法确认恢复结果。".into()
                },
                None,
            );
        }
        if let Err(error) = rpc
            .request("EditEnd", json!({"Cancel": false}))
            .await
            .and_then(require_true)
        {
            return result(
                EditorEditOutcome::Unknown,
                format!("无法确认 Editor 是否提交：{error}"),
                None,
            );
        }
        match verification_snapshot(rpc, &plan.method, &plan.data).await {
            Ok(snapshot) => match verify_postcondition(plan, &snapshot) {
                Some(true) => result(
                    EditorEditOutcome::Committed,
                    "Editor 已提交，回读语义验证通过。".into(),
                    Some(sanitize_response(snapshot)),
                ),
                Some(false) => result(
                    EditorEditOutcome::Unknown,
                    "Editor 已结束事务，但回读结果与预览不一致。".into(),
                    Some(sanitize_response(snapshot)),
                ),
                None => result(
                    EditorEditOutcome::Unknown,
                    "Editor 已结束事务，但该参数组合无法可靠回读验证。".into(),
                    Some(sanitize_response(snapshot)),
                ),
            },
            Err(error) => result(
                EditorEditOutcome::Unknown,
                format!("Editor 已结束事务，但回读验证失败：{error}"),
                None,
            ),
        }
    }

    async fn finish_editor_edit(
        &self,
        app: &AppHandle,
        operation_id: &str,
        result: EditorEditResult,
    ) {
        {
            let mut inner = self.inner.lock().await;
            if inner
                .operation
                .as_ref()
                .map(|operation| operation.id.as_str())
                == Some(operation_id)
            {
                inner.operation = None;
            }
            inner
                .editor_edit_results
                .insert(operation_id.into(), result.clone());
            if inner.editor_edit_results.len() > 32 {
                if let Some(key) = inner.editor_edit_results.keys().next().cloned() {
                    inner.editor_edit_results.remove(&key);
                }
            }
            if inner.rpc.is_some() && inner.model_uid.is_some() {
                inner.snapshot.state = EditorConnectionState::Ready;
                inner.snapshot.capabilities.batch_create_parameters = true;
                inner.snapshot.capabilities.find_part_parameters = true;
                inner.snapshot.capabilities.official_api = true;
                inner.snapshot.capabilities.official_edit_api = true;
                inner.snapshot.message = result.message.clone();
            }
        }
        self.emit_snapshot(app).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
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
                assert_eq!(request["Data"], expected_data);
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

    #[test]
    fn catalog_covers_every_official_method_without_exposing_session_primitives() {
        let exposed = tool_specs()
            .into_iter()
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
            "GetSelectedObjecs",
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
            &spec,
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
        let port = sequence_server(vec![
            (
                "GetParameterStructure",
                json!({"ModelUID": "private-model"}),
                before.clone(),
            ),
            (
                "GetCurrentModelUID",
                json!({}),
                json!({"ModelUID": "private-model"}),
            ),
            (
                "GetParameterStructure",
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
            (
                "EditSendLog",
                json!({"Message": "正在执行已确认的模型编辑。"}),
                json!({}),
            ),
            (
                "EditParameter",
                json!({
                    "ModelUID": "private-model",
                    "Id": "ParamFace",
                    "Name": "Face",
                    "IsRepeat": true
                }),
                json!({"Result": true}),
            ),
            ("EditSendProgress", json!({"Value": 1.0}), json!({})),
            ("EditEnd", json!({"Cancel": false}), json!({"Result": true})),
            (
                "GetParameterStructure",
                json!({"ModelUID": "private-model"}),
                after,
            ),
        ])
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
}
