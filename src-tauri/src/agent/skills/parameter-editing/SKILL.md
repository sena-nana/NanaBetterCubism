---
name: parameter-editing
description: Control temporary parameter values or preview, confirm, execute, cancel, and verify parameter, group, and key edits.
---

# Parameter Editing

1. Call `get_editor_snapshot` and inspect the relevant parameters, groups, keys, and target objects before editing.
2. `set_parameter_values` and `clear_parameter_values` affect only the Editor temporary parameter buffer; never describe them as saved structural edits.
3. Use `preview_parameter_batch` before the legacy parameter batch executor, then poll `get_parameter_batch_result` until it is no longer running. For official parameter, group, and key mutations, group every same-method change into one matching `preview_*` call with `operations`; even a single change uses a one-item array.
4. Gather every batch preview needed this turn, summarize all previews together, and obtain a single round approval via ask_user describing all operations at once, unless the current request already authorizes that exact set of changes.
5. After round approval, call `execute_editor_edit` once per confirmed same-method batch, then poll `get_editor_edit_result` until it is no longer running. Report success only when the entire batch is committed and every item is verified. Do not split a batch into per-item transactions or ask again per edit within the same round.
6. On cancellation, disconnect, timeout, or unknown commit/rollback state, report the real outcome and never retry a mutation automatically.
7. Only when either result returns `canOfferProjectMemory: true`, call `ask_user` once with `保存到项目记忆` and `暂不保存`. On save, read `memory-recall`, recall the project topic, then read and follow `project-memory`; store only verified fixed-layer facts, never operation handles, session UIDs, or raw RPC. On decline/cancel or a false flag, do not offer or write memory.
