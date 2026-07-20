---
name: lilia-app-design
description: Design and interaction standards for final Lilia desktop apps consuming @lilia/ui. Use for pages, sidebar entries, empty/loading/error states, cards, dialogs, menus, scoped styles, visual hierarchy, or any app UI behavior.
---

# Lilia App Design

## MUST

- MUST design final apps as restrained engineering tools. Every main view must answer: Where am I? What is the current state? Does the user need to act?
  - Reason: 缺少定位与状态会让用户在重复工作中迷失。
- MUST use `@lilia/ui` for desktop shell, titlebar, sidebar, settings page, menu language, theme, and page base classes. Put final app business pages in the main workspace only.
  - Reason: 重建壳层会导致规则漂移与重复维护。
- MUST configure navigation, footer status, and settings entry through `src/app.config.ts`; connect pages through `src/routes.ts`, preferably with async imports.
  - Reason: 集中配置让导航与状态可维护。
- MUST use CSS variables from `@lilia/ui/styles.css`. MUST NOT create a second public color system inside the final app.
  - Reason: 双色彩系统会导致主题不一致与维护负担。
- MUST give rows and controls clear hover, active, muted, disabled, loading, empty, and error states without layout shift.
  - Reason: 状态切换跳变会误触与干扰重复操作。
- MUST keep hover/active/border/text transitions around 0.12s and respect `prefers-reduced-motion: reduce`.
  - Reason: 过强动效分散注意力且影响可访问性。

## MUST NOT

- MUST NOT show technical implementation notes, roadmap placeholders, or UI that looks functional but is not wired to real behavior.
  - Reason: 占位 UI 让用户与 Agent 误判功能可用性。
- MUST NOT use landing-page hero blocks, oversized headings, decorative panels, marketing card streams, or nested cards.
- MUST NOT leak debug-only Agent affordances or technical copy into production UI.

## SHOULD

- Use cards only for independent info groups, repeated items, dialogs, and real tool containers.
- Use LiliaUI `.page-header`, `.card`, `.kv` page classes before adding local CSS; keep app-specific CSS in business component scoped styles.

## See also

- `$lilia-app-boundary` for ownership, `$lilia-agent-debug` for Agent-operated flows.
