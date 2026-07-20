---
name: nanabettercubism-app
description: Implement NanaBetterCubism across Vue and Tauri. Use for pages, routes, commands, events, Rust state, connection/permission UX, capability gating, edit previews, progress, cancellation, settings, persistence, or errors.
---

# NanaBetterCubism App

## MUST

- MUST keep connection, token, version, requests, reconnects, UIDs, and transactions in Rust. Vue receives typed domain DTOs and events, never protocol JSON.
  - Reason: 前端持有协议状态破坏授权边界与断连恢复。
- MUST model disconnected, connecting, awaiting permission, ready, editing, cancelling, incompatible, and failed states. Ignore events from superseded connections or operations.
  - Reason: 旧连接事件会污染当前状态机。
- MUST derive page and shared-shell status from the same state. Gate every action by capabilities, document, selection, and transaction; never show static success.
  - Reason: 双源状态会不一致；未门控动作会触发不可用操作。
- MUST preview destructive/bulk changes, show real progress, and label unknown commit/rollback outcomes truthfully. Say "authorization required" unless the Editor proves denial.
  - Reason: 伪报与占位让用户与 Agent 误判结果。
- MUST change Rust handlers, Tauri permissions, TypeScript types, Vue state, and functional tests together in one commit.
  - Reason: 跨端契约不同步会导致运行时类型或权限错误。

## SHOULD

- Use `$lilia-app-boundary` and `$lilia-app-design` for shared ownership and UI decisions.

## See also

- `$nanabettercubism-validation` for validation, `$cubism-edit-transactions` for mutation lifecycle.
