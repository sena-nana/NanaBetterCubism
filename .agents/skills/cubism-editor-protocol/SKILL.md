---
name: cubism-editor-protocol
description: Design Cubism Editor WebSocket transport and session lifecycle. Use for plugin registration, token permission, API version negotiation, request correlation, reconnects, timeouts, UID lifetime, or capability gating.
---

# Cubism Editor Protocol

## MUST

- MUST read `references/capability-matrix.md` before adding any method, payload, version gate, or support claim. Implement only confirmed entries.
  - Reason: 未确认方法会触发 Editor 拒绝或静默失败。
- MUST keep WebSocket、令牌、协商版本、pending 请求、重连策略、事务状态、会话 UID in Rust. Expose typed domain commands and events, never raw RPC.
  - Reason: 前端持有协议状态会破坏授权边界与断连恢复。
- MUST register → wait for permission with a bounded cancellable check → negotiate a supported version → publish capabilities. Treat a replaced token as "authorization required" unless the Editor explicitly says otherwise.
  - Reason: 跳序会导致未授权请求被拒且状态不可恢复。
- MUST treat UIDs as connection-scoped: clear UIDs and pending requests on disconnect, ignore late responses, re-query the document after reconnect, never resume or replay a mutation.
  - Reason: 旧 UID 在新连接引用错误对象或重复写入。
- MUST persist tokens only in backend-only storage or OS credentials; MUST redact tokens and model payloads from logs.
  - Reason: 日志泄漏令牌等于授权泄漏。

## SHOULD

- Retry only retryable transport failures with bounded backoff.
- Fail closed on permission, version, protocol, or unknown mutation outcomes.

## See also

- `$cubism-edit-transactions` for mutations, `$nanabettercubism-app` for Tauri/Vue wiring, `$nanabettercubism-validation` for tests.
