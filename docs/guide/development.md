# 开发与验证

## 产品定位

NanaBetterCubism 是面向 Cubism Editor 的 Agent 桌面应用（LiliaCode 特化精简版）。主界面为对话侧栏与聊天工作区；Cubism 能力仅通过 Agent 工具调用，不提供独立参数业务页。

## 边界

- Vue 只调用类型化 Tauri 命令（对话、记忆、LLM 配置、Editor 连接领域命令）。
- WebSocket、令牌、ModelUID、编辑事务留在 Rust；前端不接触原始 RPC。
- Agent ReAct 循环、API Key（keyring）、SQLite 会话/记忆均在 Rust。
- 提供 `ask_user` / `update_plan`；不做权限批准门，不提供文件直接编辑工具。

## 模块

| 路径 | 职责 |
|------|------|
| `src/features/agent/` | 对话、记忆、侧栏会话、模型/Editor 设置 |
| `src/features/editor/` | Editor 连接状态与设置卡片 |
| `src-tauri/src/agent/` | SQLite、ReAct、工具、截屏 |
| `src-tauri/src/service.rs` | Cubism Editor 会话与事务 |

## 本地运行

```bash
corepack enable
yarn install
yarn liliaui:local   # 开发期对齐本地 LiliaUI（侧栏 groups 可导航）
yarn tauri:dev
```

在设置中配置 OpenAI 兼容 API，并连接 Cubism Editor External API。

## 验证

```bash
yarn test
yarn build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
