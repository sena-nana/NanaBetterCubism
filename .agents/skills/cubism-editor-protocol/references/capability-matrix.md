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

## Provisional 5.4 Alpha / API 1.1.0 Leads

- Edit approval, transaction begin/end, progress, logs, cancellation, and undo cancellation.
- Parameter/group CRUD, parameter keys, object associations, selection, Parts, hierarchy, Deformers, and supported display properties.

Promote a lead only after recording the official source, Editor build, API version, exact schema, mode, errors, and preferably a real-Editor observation.

Until confirmed, do not expose ArtMesh geometry, Warp control points, animation, physics, save/export, texture atlas, PSD, Glue creation, or ArtPath creation.
