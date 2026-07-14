---
name: cubism-edit-transactions
description: Design safe Cubism Editor model mutations. Use for edit approval, transaction begin/end, batch edits, progress, cancellation, rollback, undo cancellation, disconnect cleanup, mutation concurrency, or any API workflow that changes CMO3 model structure.
---

# Cubism Edit Transactions

1. Confirm every mutation and lifecycle method in the protocol capability matrix; otherwise disable the workflow.
2. Build an idempotent edit plan with target document, stable IDs, preconditions, operations, and a user-readable preview.
3. Allow one backend-owned transaction at a time. Begin before the first mutation, check cancellation between bounded operations, and report real progress.
4. Commit only after all operations are acknowledged and local invariants pass. Do not assume uncommitted changes are readable.
5. Cancel on user request, validation/RPC failure, timeout, or shutdown when the confirmed protocol permits it. On disconnect or uncertain completion, mark the outcome unknown and never retry automatically.
6. Clear ownership in one cleanup path, then re-read affected objects and verify semantic postconditions before reporting success or rollback.

Use `$cubism-model-editing` for operation order and `$nanabettercubism-validation` for failure paths.
