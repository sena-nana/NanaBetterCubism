# Cubism Editor External API Capability Matrix

Last checked: 2026-07-15

Only the methods and fields below are implementation contracts. Sources are the
[stable function list](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api-list/),
the [transport and authentication manual](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api/),
the [Cubism 5.4 Alpha1 developer manual](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html),
and the official [54alpha EditSample](https://github.com/Live2D-Garage/CubismExternalAppPluginSamples/tree/54alpha/04_EditSample).

## Transport and session

| Behavior | Confirmed contract |
| --- | --- |
| Transport | WebSocket carrying UTF-8 JSON; default port 22033 is configurable. |
| Envelope | `Version`, `Timestamp?`, `RequestId?`, `Type`, `Method`, and `Data`. API 1.1.0 requests use `Version: "1.1.0"` and correlate by `RequestId`. |
| Registration | `RegisterPlugin({Name, Token?, Icon?, Path?}) -> {Token}`. A replaced token requires access authorization again. |
| Access | `GetIsApproval({}) -> {Result}`. |
| Version | `SetGlobalVersion({Version?})`; NanaBetterCubism selects `1.1.0` after access approval. |
| Edit access | API 1.1.0 adds `GetIsEditApproval({}) -> {Result}`. Edit APIs require Modeling mode and edit approval. |
| Identity | Model and document UIDs are connection-scoped. Parameter and object IDs are model-owned identifiers. UIDs and tokens remain in Rust. |
| Transaction | `EditBegin({Silent?}) -> {Result}` starts an edit. `EditEnd({Cancel?}) -> {Result}` commits or, when cancellation is confirmed, restores the pre-edit state. |
| Edit feedback | `EditSendLog({Message})`, `EditSendProgress({Value: 0..1})`, and `NotifyUndoCancel({Enabled}) -> {Accepted}` with event `{Result}`. |

## Stable instructions

The following methods are available through API 0.9.x to 1.0.1 and remain
addressable with the selected 1.1.0 envelope.

| Method | Since | Request fields | Response fields |
| --- | --- | --- | --- |
| `GetParameterValues` | 0.9.0 | `ModelUID`, `Ids?` | `Parameters[{Id, Value}]` |
| `SetParameterValues` | 0.9.0 | `ModelUID`, `Parameters[{Id, Value}]` | none |
| `GetParameters` | 0.9.0; keyforms in 1.0.1 | `ModelUID?`, `DocumentUID?` with at least one | `Parameters[{Id, Name, GroupUID, Default, Max, Min, Repeat, Type, Keyform?}]` |
| `GetParameterGroups` | 0.9.0 | `ModelUID?`, `DocumentUID?` with at least one | `Groups[{GroupUID, GroupName}]` |
| `GetDocuments` | 0.9.0; animation documents in 0.9.1 | none | physics/modeling/animation document arrays |
| `GetDocument` | 0.9.3 | `DocumentUID` | one optional physics/modeling/animation document array |
| `GetCurrentDocumentUID` | 0.9.3 | none; the stable table's request-cell `DocumentUID` is inconsistent with the method description and 5.1 release note | `DocumentUID` |
| `GetCurrentModelUID` | 0.9.0 | none | `ModelUID` |
| `GetCurrentEditMode` | 0.9.0 | none | `EditMode` |
| `ClearParameterValues` | 0.9.1 | `ModelUID` | none |
| `GetPhysicsInfo` | 0.9.2 | `ModelUID`, `Fps?` | none in the official table |
| `SendCubismLog` | 0.9.3 | `Type?: info|warning`, `Message` up to 5000 characters, `Display?` | none |

`SetParameterValues` and `ClearParameterValues` operate on the Editor's
temporary external-parameter buffer; they are not structural model edits.

## Stable event subscriptions

| Method | Since | Request | Response | Event |
| --- | --- | --- | --- | --- |
| `NotifyPhysicsFileExported` | 0.9.0 | `Enabled` | `Accepted` | `Path`, `ModelFilePath` |
| `NotifyMocFileExported` | 0.9.0; `Files` in 0.9.3 | `Enabled` | `Accepted` | `Path`, `ModelFilePath`, `Files?` |
| `NotifyMotionFileExported` | 0.9.0 | `Enabled` | `Accepted` | `Path`, `ModelFilePath` |
| `NotifyMotionSyncFileExported` | 0.9.0 | `Enabled` | `Accepted` | `Path`, `ModelFilePath` |
| `NotifyChangeEditMode` | 0.9.0 | `Enabled` | `Accepted` | `EditMode` |

## API 1.1.0 parameter and key editing

All mutation methods in this and following sections require `EditBegin` and a
successful `EditEnd`. All model-scoped methods require `ModelUID`.

| Method | Request fields after `ModelUID` | Response |
| --- | --- | --- |
| `AddParameterKey` | `ObjectId`, `ParameterId`, `KeyValue` | `Result` |
| `DeleteParameterKey` | `ObjectId?`, `ParameterId?`, `Strict?` default true, `KeyValue?` | `Result` |
| `MoveParameterKey` | `ObjectId?`, `ParameterId?`, `FromValue`, `ToValue`, `Strict?` default true, `ForceOverwrite?` default false | `Result` |
| `GetParameterKeys` | `ObjectId` | `Parameters[{Id, KeyValues}]` |
| `GetObjectsByParameterKeys` | `ParameterId`, `KeyValue` | `Ids` |
| `GetParameterStructure` | none | recursive `ParameterStructure` with groups, parameters, ranges, repeat/blend-shape state, keys, and label colors |
| `AddParameter` | `Name?`, `Id?`, `GroupId?`, `Min?`, `Default?`, `Max?`, `IsBlendShape?` | `Result` |
| `AddParameterGroup` | `Name?`, `Id?` | `Result` |
| `EditParameter` | `Id`, `NewId?`, `Name?`, `Min?`, `Default?`, `Max?`, `IsRepeat?` | `Result` |
| `EditParameterGroup` | `Id`, `NewId?`, `Name?`, `LabelColorType?`, `LabelCustomColor?` | `Result` |
| `DeleteParameter` | `Id` | `Result` |
| `DeleteParameterGroup` | `Id` | `Result` |
| `MoveParameter` | `Id`, `GroupId`, `InsertIndex?` | `Result` |
| `MoveParameterGroup` | `Id`, `InsertIndex` | `Result` |

## API 1.1.0 selection, Part, and object editing

The official method name is misspelled `GetSelectedObjecs`; clients must use
that exact spelling.

| Method | Request fields after `ModelUID` | Response |
| --- | --- | --- |
| `GetSelectedObjecs` | none | `Ids` |
| `AddSelectedObjects` | `Ids?` | `Result` |
| `ClearSelectedObjects` | none | `Result` |
| `GetPartStructure` | none | recursive `PartStructure{Name, Id, Type, Children}` |
| `GetObject` | `Id`, `Parameters?` where each filter may contain `Id?` and/or `Value?` | `Result`, `Type`, type-specific `Data` |
| `DeleteObject` | `Id` | `Result` |
| `MoveObjectOnPartsPalette` | `Id`, `ParentId?`, `InsertId?`, `InsertIndex?` | `Result` |
| `AddPart` | `Name?`, `Id?`, `DrawOrder?: 0..1000`, `Ids?`, `IsNested?` | `Result` |
| `EditPart` | `Id`, `Parameters?`, `isExactMatch?`, `NewId?`, `Name?`, `ParentId?`, `IsGrouped?`, `IsGuidImage?`, `IsOffscreen?`, `ClippingIds?`, `IsReverseMask?`, `DrawOrder?: 0..1000`, `Opacity?: 0..100`, colors/blends/label fields | `Result` |
| `EditArtMesh` | `Id`, `Parameters?`, `IsExactMatch?`, `NewId?`, `Name?`, `ParentId?`, `ParentDeformerId?`, clipping/mask/order/opacity/color/blend/culling/label fields | `Result` |
| `EditGlue` | `Id`, `Parameters?`, `IsExactMatch?`, `NewId?`, `Name?`, `ParentId?`, `Intensity?: 0..100`, label fields | `Result` |

`GetObject` supports ArtMesh, Part, WarpDeformer, RotationDeformer, and Glue
data. The manual explicitly says ArtPath acquisition is unsupported. ArtMesh
editing covers only the documented display/parent properties; it does not
expose mesh geometry, vertices, UVs, or topology. Glue creation is not exposed.

## API 1.1.0 Deformer editing

| Method | Request fields after `ModelUID` | Response |
| --- | --- | --- |
| `GetDeformerStructure` | none | recursive `DeformerStructure{Name, Id, Type, Children}` |
| `AddRotationDeformer` | `Name?`, `Id?`, `ParentId?`, `TargetObjectIds?`, `Mode?: AsParent|AsChild` | `Result` |
| `AddWarpDeformer` | rotation-add fields plus `WarpDivH/V?: 2..100`, `BezierDivH/V?: 1..100`, `ConsiderChildKeyforms?`, `SnapCenter?` | `Result` |
| `EditRotationDeformer` | `Id`, `Parameters?`, `isExactMatch?`, `NewId?`, `Name?`, `ParentId?`, `ParentDeformerId?`, angle/base-angle/scale/opacity/color/label fields | `Result` |
| `EditWarpDeformer` | `Id`, `Parameters?`, `isExactMatch?`, `NewId?`, `Name?`, `ParentId?`, `ParentDeformerId?`, opacity/color/division/label fields | `Result` |

For add-deformer `Mode=AsChild`, the official manual requires exactly one
target object. `SnapCenter=true` is valid only with
`ConsiderChildKeyforms=true`.

## Enumerations and documented inconsistencies

- Label colors: `Undefined`, `Red`, `Orange`, `Yellow`, `Green`, `Blue`,
  `Purple`, `Gray`, `Custom`; custom colors are `#000000` to `#FFFFFF`.
- Color blends: `Normal`, `Add`, `AddGlow`, `Darken`, `Multiply`, `ColorBurn`,
  `LinearBurn`, `Lighten`, `Screen`, `ColorDodge`, `Overlay`, `SoftLight`,
  `HardLight`, `LinearLight`, `Hue`, `Color`, `Add_5.2`, `Multiply_5.2`.
- Alpha blends: `Over`, `Atop`, `Out`, `Conjoint`, `Disjoint`.
- The Alpha1 manual's `EditPart` request summary contains `ParentID` and
  `Arrat` typos. Its field definitions use `ParentId` and Array, consistent
  with the other object methods; the implementation follows the field
  definitions.
- The Alpha1 manual varies `isExactMatch` casing by method. The implementation
  preserves its request-table casing: lower-case initial for EditPart and both
  Deformer edits, upper-case initial for EditArtMesh and EditGlue.

## Unsupported

Do not infer or expose ArtMesh geometry, UV/topology changes, Warp control
points, animation editing, physics editing, save/export commands, texture
atlas, PSD operations, Glue creation, ArtPath creation/acquisition, or any API
not listed above.
