# NanaBetterCubism Agent Spec

> NanaBetterCubism = Tauri/Vue app that extends Cubism Editor via WebSocket + JSON.
> In scope: Editor connection & editing flow. Out of scope: Cubism Core, MOC3 runtime, rendering, `live2d-rs`.

## MUST

- MUST verify every API method/version against `references/capability-matrix.md` before implementing or claiming support. 聊天与 Alpha 摘要仅作待验证线索。
  - Reason: Editor 协议随版本变化，未确认能力会导致运行时崩溃或伪成功。
- MUST keep WebSocket、令牌、版本协商、请求、事务、会话 UID in Rust; 前端只消费类型化领域命令与事件。
  - Reason: 暴露令牌或原始 RPC 破坏授权边界与可恢复性。
- MUST NOT persist session UIDs; MUST NOT expose tokens or raw RPC to the frontend.
  - Reason: UID 跨连接复用会引用已失效对象，令牌泄漏破坏授权。
- MUST use a confirmed edit transaction for every model write. On failure/cancel/timeout/disconnect, MUST NOT report commit or rollback success.
  - Reason: 模型写操作不可逆，伪报成功污染用户模型。
- MUST NOT claim ArtMesh geometry, animation, physics, save/export, texture atlas, or PSD support without official-doc + real-Editor verification.
  - Reason: 这些能力未在当前协议确认，臆造会写入无法运行的代码。
- UI MUST show only real state and wired capabilities. MUST NOT show static success, placeholder actions, or technical notes.
  - Reason: 占位状态让用户与 Agent 误判功能可用性。
- LiliaUI owns shared shell/settings/theme/components; this repo owns Cubism business capabilities. 跨端共享时先在 LiliaUI 定义公共接口。
  - Reason: 双向复制会导致规则漂移与重复维护。
- Tests MUST assert behavior and state recovery. MUST NOT hard-match logs or copy. MUST NOT overwrite user or other-agent changes.
  - Reason: 硬匹配日志/文案会因无关改动而误判；覆盖他人改动破坏协作。

## SHOULD

- 跨端契约变更时，同一提交内同步改 Rust handler、Tauri 权限、TS 类型、Vue 状态与功能测试。
- 多文件重构前先建 git checkpoint（见 `$lilia-app-git`）。

## Skills

- `$cubism-editor-protocol` — 连接/授权/版本/请求/UID 生命周期
- `$cubism-edit-transactions` — 可取消可回滚的模型编辑事务
- `$cubism-model-editing` — 参数/关键点/Part/对象/Deformer 操作
- `$nanabettercubism-app` — Vue/Tauri 业务功能与类型化契约
- `$nanabettercubism-validation` — 模拟 Editor/契约测试/真实联调
- `$lilia-app-design` — 应用设计与交互
- `$lilia-app-boundary` — 应用与 LiliaUI 所有权边界
- `$lilia-app-git` — Git 收口
- `$lilia-agent-debug` — Agent 调试与桌面验证
