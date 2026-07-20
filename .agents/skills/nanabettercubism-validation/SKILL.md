---
name: nanabettercubism-validation
description: Validate NanaBetterCubism protocol and application behavior. Use for JSON fixtures, mock Editor servers, permission/version negotiation, Tauri contracts, Vue state, cancellation, rollback, reconnects, capability gates, or real-Editor smoke tests.
---

# NanaBetterCubism Validation

## MUST

- MUST unit-test parsing, capability derivation, edit planning, and state transitions.
  - Reason: 这些纯函数是协议正确性的基础。
- MUST use a mock WebSocket Editor for registration, permission waits, version mismatch, correlation, malformed responses, timeout, reconnect, stale UID, cancellation, and disconnect-during-mutation.
  - Reason: 真实 Editor 无法稳定复现这些故障路径。
- MUST test Tauri/Vue at the typed domain boundary; assert capabilities, state, errors, cancellation, reconciliation, and semantic postconditions instead of logs, copy, timing, or JSON order.
  - Reason: 硬匹配日志/文案会因无关改动误判。
- MUST version and sanitize fixtures. MUST NOT store tokens, private model data, or provisional Alpha schemas as conformance fixtures.
  - Reason: 敏感数据进仓库会泄漏；provisional schema 会固化未确认协议。
- MUST run real-Editor mutation checks only against disposable model copies. Record Editor build, API version, OS, fixture, and outcome; report them separately from automated coverage.
  - Reason: 真机写入不可逆，且结果不可在 CI 复现。

## See also

- `$cubism-editor-protocol` and `$cubism-edit-transactions` for the behavior under test.
