# Agent 入口规范

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- MCP tools (when available): `codegraph_explore` answers most code questions in one call, returning relevant source plus call paths. `codegraph_node` returns one symbol's source and callers, or reads a whole file with line numbers. If the tools are deferred, load them by name via tool search.
- Shell fallback: `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely.
<!-- CODEGRAPH_END -->

## 项目级 Skills

本仓库通过 `.agents/skills` 为基于模板创建的最终应用提供 Agent 能力。处理对应任务时优先使用这些 Skill,不要把细则继续堆进 `AGENTS.md`。

- `$lilia-app-design`: 设计、交互、视觉层级、页面样式、侧边栏、卡片、浮层和状态评审。
- `$lilia-app-coding`: 功能实现、问题修复、重构、路由、命令、业务页面和应用专属 Tauri 代码。
- `$lilia-app-boundary`: 判断改动属于最终应用还是 LiliaUI 公共能力。
- `$lilia-app-validation`: 选择功能验证、测试、构建、Tauri 检查和结果汇报方式。
- `$lilia-app-git`: 暂存、提交、推送、合并和依赖更新收口。
- `$lilia-agent-debug`: Agent 调试入口、`data-agent-id`、`window.__liliaAgentDebug`、`yarn agent:debug` 和 `tauri-driver` 调试验证。

## 硬约束

- 灵活运用子代理任务分派,并行化执行边界清晰的任务;主 Agent 负责整合、验证和收口。
- 修复问题时先定位根本原因,禁止打补丁式修复。
- 实现前结合上下文判断代码和设计是否有足够价值,优先选择更简洁优雅的方案。
- 禁止在 UI 显示技术说明内容。
- 禁止让 UI 看起来像有功能但实际未接入;所有可见操作必须落地功能或表达真实不可用状态。
- 禁止添加低价值测试和硬匹配日志或字符串的测试;所有测试必须以功能为准,无功能变动则不添加测试。
- 不覆盖用户或其他 Agent 的已有改动。
