---
name: object-editing
description: Preview, confirm, execute, cancel, and verify selection, Part, object, ArtMesh property, Glue property, and Deformer edits.
---

# Object Editing

1. Call `get_editor_snapshot`, then inspect selection, Part/Object hierarchy, Deformer hierarchy, and affected properties before editing.
2. Compute the smallest change that preserves unrelated state. Require explicit intent for deletion, bulk movement, or destructive normalization.
3. Group every same-method change into one matching `preview_*` call with `operations`; even a single change uses a one-item array. Gather every batch preview needed this turn, summarize all previews together, and obtain a single round approval via ask_user describing all operations at once, unless the current request already authorizes that exact set of changes.
4. After round approval, call `execute_editor_edit` once per confirmed same-method batch, then poll `get_editor_edit_result`. Report success only when the entire batch is committed and every item is verified. Do not split a batch into per-item transactions or ask again per edit within the same round.
5. On cancellation, disconnect, timeout, or unknown commit/rollback state, report the real outcome and never retry automatically.
6. Do not infer or promise mesh geometry, UV/topology, Warp control-point, animation, physics-editing, save/export, atlas, PSD, Glue-creation, or ArtPath capabilities.
7. Only when `get_editor_edit_result` returns `canOfferProjectMemory: true`, call `ask_user` once with `保存到项目记忆` and `暂不保存`. On save, read `memory-recall`, recall the project topic, then read and follow `project-memory`; store only verified fixed-layer facts, never operation handles, session UIDs, or raw RPC. On decline/cancel or a false flag, do not offer or write memory.
