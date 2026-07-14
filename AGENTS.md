@/Users/zt-c604184/.codex/RTK.md

# NanaBetterCubism Agent 规范

NanaBetterCubism 是通过 WebSocket + JSON 扩展 Cubism Editor 建模能力的 Tauri/Vue 应用。Editor 连接和编辑流程属于本仓库；Cubism Core、MOC3 运行时、渲染和 `live2d-rs` 不属于本仓库。

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- MCP tools (when available): `codegraph_explore` answers most code questions in one call, returning relevant source plus call paths. `codegraph_node` returns one symbol's source and callers, or reads a whole file with line numbers. If the tools are deferred, load them by name via tool search.
- Shell fallback: `codegraph explore "<symbol names or question>"` and `codegraph node <symbol-or-file>` print the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely.
<!-- CODEGRAPH_END -->

## Skills

- `$cubism-editor-protocol`: 连接、授权、版本、请求和 UID 生命周期。
- `$cubism-edit-transactions`: 可取消、可回滚的模型编辑事务。
- `$cubism-model-editing`: 参数、关键点、Part、对象和 Deformer 操作。
- `$nanabettercubism-app`: Vue/Tauri 业务功能与类型化契约。
- `$nanabettercubism-validation`: 模拟 Editor、契约测试和真实联调。
- `$lilia-app-design`: 应用设计与交互。
- `$lilia-app-boundary`: 应用与 LiliaUI 的所有权边界。
- `$lilia-app-git`: Git 收口。
- `$lilia-agent-debug`: Agent 调试与桌面验证。

## 硬约束

- 先核对官方资料和版本化能力矩阵；聊天与 Alpha 摘要只能作为待验证线索。
- Rust 后端持有 WebSocket、令牌、版本协商、请求和事务；前端只使用类型化领域命令与事件。
- 不持久化会话 UID，不向前端暴露令牌或原始 RPC。
- 模型写操作必须使用当前版本确认支持的编辑事务；失败、取消、超时或断连时禁止伪报提交或回滚成功。
- 未经官方资料和真实验证确认，不提供 ArtMesh 几何、动画、物理、保存、导出、纹理图集或 PSD 能力。
- UI 只能展示真实状态和已接入功能；禁止静态成功状态、占位操作和技术说明。
- LiliaUI 拥有共享壳层、设置、主题和公共组件；本仓库拥有 Cubism 业务能力。
- 测试行为和状态恢复，不硬匹配日志或文案；不覆盖用户或其他 Agent 的改动。
