---
name: lilia-app-boundary
description: Ownership rules for deciding whether a final Lilia app change belongs in the app repo or in LiliaUI. Use for shell, titlebar, sidebar, settings, menus, theme, global CSS, config sync, build wrappers, default assets, window state, @lilia packages, routes, commands, or business pages.
---

# Lilia App Boundary

## MUST

- MUST put behavior in the final app only when it is application-specific business logic, app config, page routing, command wiring, or an app-owned Tauri boundary.
  - Reason: 应用特有逻辑进 LiliaUI 会污染共享层。
- MUST move or implement behavior in LiliaUI when it is reusable shell, UI system, styling, config, tooling, build, template check, default asset, or common Tauri runtime behavior.
  - Reason: 在 app 内复制共享行为会导致规则漂移。
- Final app owns: `app.config.json`, `src/app.config.ts`, `src/routes.ts`, `src/commands.ts`, `src/features/**`, `src-tauri/**`, `tests/**`.
- LiliaUI owns: `@lilia/ui`, `@lilia/config`, `@lilia/tools`, `@lilia/build`, `tauri-plugin-lilia`.
- If both sides must change, MUST define the public LiliaUI/component or debug interface first, then update LiliaUI and the final app in that order.
  - Reason: 接口先行避免 app 依赖 LiliaUI 私有实现。

## MUST NOT

- MUST NOT edit `node_modules/@lilia/*`. Modify the LiliaUI source repo, validate there, then update the app dependency or lockfile.
  - Reason: 直接改 node_modules 会被下次安装覆盖。
- MUST NOT copy Lilia-specific paths, protocols, providers, task timelines, or verification scripts into final apps unless the app truly implements that capability.
- MUST NOT duplicate shared shell or style code locally for a quick fix.

## SHOULD

- When unsure, inspect the current app code and LiliaUI package surface before choosing a boundary.
- Repeated style or component pattern across final apps: implement in LiliaUI. One-off business visualization: keep scoped in the app component.

## See also

- `$lilia-agent-debug` for Agent debug workflows, `$lilia-app-design` for UI standards.
