---
name: nanabettercubism-app
description: Implement NanaBetterCubism across Vue and Tauri. Use for pages, routes, commands, events, Rust state, connection and permission UX, capability gating, edit previews, progress, cancellation, settings, persistence, errors, or Cubism-specific cross-end contracts.
---

# NanaBetterCubism App

- Use CodeGraph first when indexed. Use `$lilia-app-boundary` and `$lilia-app-design` for shared ownership and UI decisions.
- Keep connection, token, version, requests, reconnects, UIDs, and transactions in Rust. Vue receives typed domain DTOs and events, never protocol JSON.
- Model disconnected, connecting, awaiting permission, ready, editing, cancelling, incompatible, and failed states. Ignore events from superseded connections or operations.
- Derive page and shared-shell status from the same state. Gate every action by capabilities, document, selection, and transaction; never show static success.
- Preview destructive/bulk changes, show real progress, and label unknown commit/rollback outcomes truthfully. Say “authorization required” unless the Editor proves denial.
- Change Rust handlers, Tauri permissions, TypeScript types, Vue state, and functional tests together. Validate with `$nanabettercubism-validation`.
