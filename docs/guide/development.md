# 开发与验证

## 边界

Vue 只调用 `connect_editor`、`disconnect_editor`、`get_editor_snapshot`、`preview_parameter_batch`、`execute_parameter_batch` 和 `cancel_parameter_batch` 领域命令。WebSocket、API 版本、令牌、请求关联、模型 UID、预览失效和编辑事务均留在 Rust；前端 DTO 不包含令牌、原始 RPC、DocumentUID 或 ModelUID。

`src/features/parameters/ParameterBatchPage.vue` 只编排工作区状态；命名预设位于 `composables`，ID 与粘贴转换位于功能内 `utils`。粘贴面板、逐行覆盖和执行状态组件按出现条件动态加载，应用路由本身也按页面拆包。跨功能复用的分隔文本和本地存储能力位于 `src/utils`。

Rust 的 `service.rs` 持有唯一 Editor 会话和事务状态机；`service/commands.rs` 适配 Tauri 命令，`credentials.rs` 负责系统凭据，`model_structure.rs` 负责结构解析与回读验证，`transaction.rs` 统一编辑请求、取消通知和结果判定。`protocol.rs` 是 WebSocket 请求关联层，`domain.rs` 负责模板展开、模型冲突和范围校验。

## 本地运行

```bash
corepack enable
yarn install
yarn tauri:dev
```

在 Cubism Editor 中启用本机 External API，打开建模模型，并授予 NanaBetterCubism 访问和编辑权限。首版不创建关键点，也不提供保存、导出、动画、物理、纹理或 PSD 操作。

## 验证

```bash
yarn test
yarn build
yarn agent:debug --json
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

`yarn agent:debug --json` 检查连接、粘贴、参数行、预览、确认、进度和取消所需的稳定 `data-agent-id`。桌面回放仍需要本机 `tauri-driver` 与平台 WebDriver；缺少这些外部工具不会影响基础就绪检查。

真实 Editor 冒烟测试应使用一次性模型副本，并记录 Editor 版本、API 版本和操作系统。分别验证正常提交后的结构回读、Editor 侧取消无残留，以及新组、普通、Blend Shape 和 Repeat 参数。
