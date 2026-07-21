---
name: object-editing
description: Preview, confirm, execute, cancel, and verify selection, Part, object, ArtMesh property, Glue property, and Deformer edits.
---

# Object Editing

1. Call `get_editor_snapshot`, then inspect selection, Part/Object hierarchy, Deformer hierarchy, and affected properties before editing.
2. Compute the smallest change that preserves unrelated state. Require explicit intent for deletion, bulk movement, or destructive normalization.
3. Gather every `preview_*` needed this turn, summarize all previews together, and obtain a single round approval via ask_user describing all operations at once, unless the current request already authorizes that exact set of changes.
4. After round approval, execute every confirmed preview with `execute_editor_edit`, poll `get_editor_edit_result`, and report success only for a verified committed outcome. Do not ask again per edit within the same round; if the plan changes mid-round, stop and re-confirm via ask_user before proceeding.
5. On cancellation, disconnect, timeout, or unknown commit/rollback state, report the real outcome and never retry automatically.
6. Do not infer or promise mesh geometry, UV/topology, Warp control-point, animation, physics-editing, save/export, atlas, PSD, Glue-creation, or ArtPath capabilities.
7. Only when `get_editor_edit_result` returns `canOfferProjectMemory: true`, call `ask_user` once with `保存到项目记忆` and `暂不保存`. On save, read `memory-recall`, recall the project topic, then read and follow `project-memory`; store only verified fixed-layer facts, never operation handles, session UIDs, or raw RPC. On decline/cancel or a false flag, do not offer or write memory.
