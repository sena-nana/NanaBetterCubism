---
name: nanabettercubism-validation
description: Validate NanaBetterCubism protocol and application behavior. Use for JSON fixtures, mock Editor servers, permission and version negotiation, Tauri contracts, Vue state, cancellation, rollback, reconnects, capability gates, real Editor smoke tests, or release verification.
---

# NanaBetterCubism Validation

- Unit-test parsing, capability derivation, edit planning, and state transitions.
- Use a mock WebSocket Editor for registration, permission waits, version mismatch, correlation, malformed responses, timeout, reconnect, stale UID, cancellation, and disconnect during mutation.
- Test Tauri/Vue at the typed domain boundary; assert capabilities, state, errors, cancellation, reconciliation, and semantic postconditions instead of logs, copy, timing, or JSON order.
- Version and sanitize fixtures. Do not store tokens, private model data, or provisional Alpha schemas as conformance fixtures.
- Run real-Editor mutation checks only against disposable model copies. Record Editor build, API version, OS, fixture, and outcome; report them separately from automated coverage.
