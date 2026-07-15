# 开发与验证

## 产品定位

NanaBetterCubism 是面向 Cubism Editor 的 Agent 桌面应用（LiliaCode 特化精简版）。主界面为对话侧栏与聊天工作区；Cubism 能力仅通过 Agent 工具调用，不提供独立参数业务页。

## 边界

- Vue 只调用类型化 Tauri 命令（对话、记忆、LLM 配置、Editor 连接领域命令）。
- WebSocket、令牌、ModelUID、编辑事务留在 Rust；前端不接触原始 RPC。
- Agent 工具使用领域参数；官方 ModelUID/DocumentUID 由后端注入或映射为连接内引用。
- 官方结构编辑 API 必须经过 preview、确认、单事务执行与语义回读；`unknown` 不能当作成功。
- Agent ReAct 循环、API Key（keyring）、SQLite 会话/记忆均在 Rust。
- 新对话由 Rust 通过当前建模文档路径自动归类；未保存或无法识别的文档进入收集箱，已有对话不会随 Editor 切换而迁移。
- 提供 `ask_user` / `update_plan`；不做权限批准门，不提供文件直接编辑工具。
- Agent 初始只获得连接、提问、计划与 `read_skill` 基础工具；运行时 SKILL 按当前回合激活领域工具，等待用户回答后继续保留，新消息开始时重置。

## 模块

| 路径 | 职责 |
|------|------|
| `src/features/agent/` | 对话、记忆、侧栏会话、模型/Editor 设置 |
| `src/features/editor/` | Editor 连接状态与设置卡片 |
| `src-tauri/src/agent/` | SQLite、ReAct、运行时 SKILL、工具门控、截屏 |
| `src-tauri/src/service.rs` | Cubism Editor 会话与事务 |
| `src-tauri/src/service/official_api.rs` | 官方 API 目录、参数校验、工具与编辑回读 |

## 本地运行

```bash
npm install --global corepack@0.35.0
corepack enable
corepack yarn install
corepack yarn liliaui:local   # 开发期对齐本地 LiliaUI（侧栏 groups 可导航）
corepack yarn tauri:dev
```

本仓库统一使用 Node.js 26.5.0 与 Yarn 4.17.1。Corepack 从 Node.js 25 起不再随 Node.js 分发，因此首次使用前需要通过 npm 显式安装 Corepack 0.35.0。

在设置中配置 OpenAI 兼容 API，并连接 Cubism Editor External API。

## 验证

```bash
yarn test
yarn build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
