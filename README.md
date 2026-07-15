# NanaBetterCubism

NanaBetterCubism 是面向 Live2D Cubism Editor 的 Agent 桌面应用（LiliaCode 特化精简版）。通过对话驱动 Cubism 工具调用，支持提问、计划、项目阶段记忆与全局 Live2D 经验；不做权限批准门，也不提供本地文件直接编辑。

## 能力

- OpenAI 兼容 ReAct Agent：按需读取内置 SKILL，仅向模型开放当前任务需要的 Cubism 领域工具与窗口截屏
- 提问（`ask_user`）与计划（`update_plan`）基础交互
- 侧栏会话列表、手绑项目名、跨会话记忆
- Cubism Editor 连接（本机 `127.0.0.1`，默认端口 `22033`）
- Cubism Editor 官方 External API 全量工具：查询、临时参数控制、文档、通知、参数/关键点、Part/Object 与 Deformer
- 所有结构编辑统一走 preview → 确认 → 后端事务 → 回读验证；支持 Agent/Editor 取消

## 开发

```bash
npm install --global corepack@0.35.0
corepack enable
corepack yarn install
corepack yarn liliaui:local
corepack yarn tauri:dev
```

本仓库统一使用 Node.js 26.5.0 与 Yarn 4.17.1。Corepack 从 Node.js 25 起不再随 Node.js 分发，因此首次使用前需要通过 npm 显式安装 Corepack 0.35.0。

在设置中配置模型 API 与 Editor 连接。

## 验证

```bash
yarn test
yarn build
yarn agent:debug --json
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
