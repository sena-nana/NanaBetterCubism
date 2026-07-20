---
name: lilia-agent-debug
description: Agent debugging workflow for final Lilia desktop apps and the Tauri template. Use for agent debug support, data-agent-id targets, window.__liliaAgentDebug, yarn agent:debug, tauri-driver readiness, debug-only UI instrumentation, or desktop replay/debug harness.
---

# Lilia Agent Debug

## MUST

- MUST treat Agent debugging as a real developer interface, not as visible product UI. Users see normal app state; Agents get stable hidden structure and dev-only debug APIs.
  - Reason: 调试 UI 泄漏到生产会干扰用户且破坏视觉一致性。
- MUST put shared frontend harness code in `@lilia/ui`; readiness/template checks and migrations in `@lilia/tools`; desktop replay orchestration, dev-server startup, `tauri-driver`, screenshots, and artifact collection in `@lilia/build`. Keep final app code limited to app-owned `data-agent-id` targets, feature scenarios, and thin script entries.
  - Reason: 分层所有权避免跨仓库复制与漂移。
- MUST gate frontend debug APIs with explicit dev/test flags such as `VITE_LILIA_AGENT_DEBUG=1` or an agent-debug mode. Debug APIs MUST NOT install in normal production UI.
  - Reason: 生产暴露调试 API 会泄漏内部结构。
- MUST name `data-agent-id` by functional path, not translated text, CSS class, DOM position, or layout: `home.start-card`, `settings.provider.save`, `tasks.row.<taskId>.open`.
  - Reason: 功能路径稳定，文案与布局会变。
- MUST keep `data-agent-id` invisible and non-semantic to users. MUST NOT add public technical instructions, automation labels, or debug-only copy to the UI.

## SHOULD

- Start with `$lilia-app-boundary` when the change crosses template, LiliaUI, Tauri, or app-owned feature code.
- Split harness work into env, types, logging, snapshots, actions, and installer modules when logic grows.
- When adding a template script, prefer a thin entry that delegates to `@lilia/tools` or `@lilia/build`; isolate compatibility fallbacks.

## Expected Interfaces

- `yarn agent:debug --json`: readiness, important files, stable targets, env flags, external tool availability.
- `window.__liliaAgentDebug.observe()`: route, viewport, active element, visible `data-agent-id` tree, recent errors.
- `window.__liliaAgentDebug.act(...)`: operate by `data-agent-id`, not by text/class/coordinate/screenshot.
- `window.__liliaAgentDebug.mark(...)`: record a debug marker without changing business data.
- `window.__liliaAgentDebug.getRecentErrors()`: recent frontend errors for debugging.

## Desktop Replay

Use `tauri-driver` for desktop automation, but do not model it as an npm dependency. Detect `tauri-driver`, EdgeDriver, or another WebDriver bridge in readiness reports. Treat missing replay tools as a setup blocker only for replay scenarios, not for basic template readiness. Keep replay scenarios functional: assert route behavior, command effects, stable targets, invoke boundaries, or persisted records; do not hard-match incidental text or logs.

## Validation

- Harness changes in LiliaUI: focused UI tests + relevant package typecheck.
- `@lilia/tools` report changes: focused tools tests + `yarn workspace @lilia/tools typecheck`.
- Template script or target changes: `yarn agent:debug --json`, `yarn test tests/tooling.test.ts`, and `yarn build` when frontend files changed.
- If a full desktop replay is expected but cannot run, report the missing tool, command, artifact path if any, and remaining risk.

## See also

- `$lilia-app-boundary` for ownership, `$lilia-app-design` for UI standards.
