---
name: cubism-edit-transactions
description: Design safe Cubism Editor model mutations. Use for edit approval, transaction begin/end, batch edits, progress, cancellation, rollback, undo cancellation, disconnect cleanup, or mutation concurrency.
---

# Cubism Edit Transactions

## MUST

- MUST confirm every mutation and lifecycle method in the protocol capability matrix; otherwise disable the workflow.
  - Reason: 未确认方法会让事务进入不可恢复状态。
- MUST build an idempotent edit plan with target document, stable IDs, preconditions, operations, and a user-readable preview.
  - Reason: 幂等计划让重试与回滚可判定，避免重复写入。
- MUST allow one backend-owned transaction at a time. Begin before the first mutation, check cancellation between bounded operations, report real progress.
  - Reason: 并发事务会让 Editor 状态与本地不变量冲突。
- MUST commit only after all operations are acknowledged and local invariants pass. MUST NOT assume uncommitted changes are readable.
  - Reason: 未 ack 的提交会留下半完成模型。
- MUST cancel on user request, validation/RPC failure, timeout, or shutdown when the confirmed protocol permits it. On disconnect or uncertain completion, MUST mark the outcome unknown and never retry automatically.
  - Reason: 不确定结果自动重试会重复写入或覆盖用户操作。
- MUST clear ownership in one cleanup path, then re-read affected objects and verify semantic postconditions before reporting success or rollback.
  - Reason: 未验证后置条件会伪报成功。

## See also

- `$cubism-model-editing` for operation order, `$nanabettercubism-validation` for failure paths.
