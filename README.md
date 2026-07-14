# NanaBetterCubism

NanaBetterCubism 是面向 Live2D Cubism Editor 的 Agent 桌面应用（LiliaCode 特化精简版）。通过对话驱动 Cubism 工具调用，支持提问、计划、项目阶段记忆与全局 Live2D 经验；不做权限批准门，也不提供本地文件直接编辑。

## 能力

- OpenAI 兼容 ReAct Agent：自动调用 Cubism 领域工具与窗口截屏
- 提问（`ask_user`）与计划（`update_plan`）基础交互
- 侧栏会话列表、手绑项目名、跨会话记忆
- Cubism Editor 连接（本机 `127.0.0.1`，默认端口 `22033`）
- 工具层保留：部件关联参数查询、批量参数创建事务（preview → execute / cancel）

## 开发

```bash
corepack enable
yarn install
yarn liliaui:local
yarn tauri:dev
```

在设置中配置模型 API 与 Editor 连接。

## 验证

```bash
yarn test
yarn build
yarn agent:debug --json
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
