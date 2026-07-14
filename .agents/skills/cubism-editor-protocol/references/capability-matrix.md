# Cubism Editor API Capability Matrix

Last checked: 2026-07-14

Use `confirmed` facts as contracts. Treat `provisional` facts only as research leads and never infer JSON fields from them.

## Confirmed

| Area | Behavior | Source |
| --- | --- | --- |
| Transport | WebSocket carrying UTF-8 JSON; default port 22033 is configurable. | [External API integration](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api/) |
| Registration | `RegisterPlugin` starts registration; Editor permission gates API use; an accepted token can be reused. | [Token authentication](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api/#token-authentication) |
| API selection | The application can select or clear the API version. Confirm exact fields before implementation. | [Function list](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api-list/) |
| Identity | UIDs may change between connections; parameter IDs remain stable unless the model changes. | [UID and parameter ID](https://docs.live2d.com/en/cubism-editor-manual/external-application-integration-api/#uid-and-parameter-id) |
| 5.4 Alpha edit permission | Alpha1 / API `1.1.0` adds a separate Edit permission and `GetIsEditApproval`; editing APIs are available only in Modeling mode. | [5.4 Alpha1 External App Integration](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html) |
| 5.4 Alpha transactions | `EditBegin` is required before editing APIs; `EditEnd` commits or cancels, and a confirmed cancel restores the pre-edit state. `EditSendProgress` and `NotifyUndoCancel` expose real progress and Editor-side cancellation. | [5.4 Alpha1 External App Integration](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html) |
| 5.4 Alpha parameters | API `1.1.0` confirms `GetParameterStructure`, `AddParameterGroup`, `AddParameter`, and `EditParameter`. `AddParameter` supports name, ID, group ID, min/default/max, and blend-shape state; repeat is set through `EditParameter`. | [5.4 Alpha1 External App Integration](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html) |
| 5.4 Alpha Part parameter lookup | API `1.1.0` confirms the read-only chain `GetSelectedObjecs({ModelUID}) -> {Ids}`, `GetPartStructure({ModelUID}) -> {PartStructure: {Name, Id, Type, Children}}`, and `GetParameterKeys({ModelUID, ObjectId}) -> {Parameters: [{Id, KeyValues}]}`. The documented errors are `UnsupportedVersion`, `InvalidData`, `InvalidModel`, and, where listed, `InvalidDocument`; these methods are in the edit API surface used after access/Edit approval in Modeling mode. | [5.4 Alpha1 External App Integration](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html) |
| 5.4 Alpha sample | The official Alpha sample sends version `1.1.0`, waits for access and Edit permission, wraps mutations in `EditBegin` / `EditEnd`, and correlates requests by ID. | [Live2D GARAGE EditSample](https://github.com/Live2D-Garage/CubismExternalAppPluginSamples/tree/54alpha/04_EditSample) |

## Provisional 5.4 Alpha / API 1.1.0 Leads

- Selection mutation, object mutation, hierarchy mutation, Deformer editing, and supported display properties beyond the confirmed read-only Part lookup above.

Promote a lead only after recording the official source, Editor build, API version, exact schema, mode, errors, and preferably a real-Editor observation.

Until confirmed, do not expose ArtMesh geometry, Warp control points, animation, physics, save/export, texture atlas, PSD, Glue creation, or ArtPath creation.
