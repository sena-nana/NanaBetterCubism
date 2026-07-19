---
name: parameter-editing
description: Control temporary parameter values or preview, confirm, execute, cancel, and verify parameter, group, and key edits.
---

# Parameter Editing

1. Call `get_editor_snapshot` and inspect the relevant parameters, groups, keys, and target objects before editing.
2. `set_parameter_values` and `clear_parameter_values` affect only the Editor temporary parameter buffer; never describe them as saved structural edits.
3. Use `preview_parameter_batch` before the legacy parameter batch executor, then poll `get_parameter_batch_result` until it is no longer running. For official parameter, group, and key mutations, use the matching `preview_*` tool.
4. Summarize the preview and obtain explicit user confirmation unless the current request already confirms that exact change.
5. Execute confirmed official previews with `execute_editor_edit`, poll `get_editor_edit_result` until it is no longer running, and report success only for a verified committed outcome.
6. On cancellation, disconnect, timeout, or unknown commit/rollback state, report the real outcome and never retry a mutation automatically.
7. Only when either result returns `canOfferProjectMemory: true`, call `ask_user` once with `保存到项目记忆` and `暂不保存`. On save, read `memory-recall`, recall the project topic, then read and follow `project-memory`; store only verified fixed-layer facts, never operation handles, session UIDs, or raw RPC. On decline/cancel or a false flag, do not offer or write memory.
