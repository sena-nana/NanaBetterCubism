---
name: cubism-editor-protocol
description: Design NanaBetterCubism's Cubism Editor transport and session layer. Use for WebSocket JSON, plugin registration, token permission, API version negotiation, request correlation, reconnects, timeouts, errors, document identity, UID lifetime, stable IDs, or capability gating.
---

# Cubism Editor Protocol

1. Read `references/capability-matrix.md` before adding a method, payload, version gate, or support claim. Implement only confirmed entries.
2. Keep the WebSocket, token, negotiated version, pending requests, reconnect policy, transaction state, and session UIDs in Rust. Expose typed domain commands and events, never raw RPC.
3. Register, wait for permission with a bounded cancellable check, negotiate a supported API version, then publish capabilities. Treat a replaced token as “authorization required” unless the Editor explicitly says otherwise.
4. Treat UIDs as connection-scoped. Clear them and pending requests on disconnect, ignore late responses, re-query the document after reconnect, and never resume or replay a mutation.
5. Persist tokens only in backend-only storage or OS credentials. Redact tokens and model payloads from logs.
6. Retry only retryable transport failures with bounded backoff. Fail closed on permission, version, protocol, or unknown mutation outcomes.

Use `$cubism-edit-transactions` for mutations, `$nanabettercubism-app` for Tauri/Vue wiring, and `$nanabettercubism-validation` for tests.
