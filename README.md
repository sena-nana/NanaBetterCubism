# NanaBetterCubism

NanaBetterCubism 是面向 Live2D Cubism Editor 的 Tauri/Vue 桌面工具。当前版本通过本机 WebSocket 直连 Cubism 5.4 Alpha1 External API 1.1.0，在一个可取消事务中批量创建参数和至多一个参数组。

## 参数批量生成

- ID 模板支持 `{prefix}`、`{key}`、`{side}`、`{index}`、`{suffix}`，默认生成 `Param{key}{side}{index}`。
- 支持本地命名预设、可编辑参数行，以及带或不带表头的 TSV/CSV 粘贴；每批最多 200 行。
- 支持批量范围、Blend Shape、Repeat 和参数组默认值，并可逐行覆盖。
- 前端即时预览，Rust 在执行前结合当前模型结构完成权威校验。
- 创建使用 `EditBegin` / `EditEnd`；取消、失败、超时和断连分别报告已回滚、失败或未知，绝不自动重放编辑。

只连接 `127.0.0.1`，默认端口为 `22033`。需要在 Cubism Editor 的外部应用联动设置中授予访问与编辑权限，并保持建模模式和当前模型可用。令牌由 Rust 保存到系统凭据库；保存失败时只在当前 Editor 会话中使用，不会返回前端或写入日志。

## 开发

```bash
corepack enable
yarn install
yarn dev
yarn tauri:dev
```

完整验证：

```bash
yarn test
yarn build
yarn agent:debug --json
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

官方依据：[Cubism 5.4 Alpha External API 手册](https://cubism.live2d.com/editor-alpha/doc/manual/alpha1/en/external-api-intergration/index.html)、[官方编辑样例](https://github.com/Live2D-Garage/CubismExternalAppPluginSamples/tree/54alpha/04_EditSample)和 [Live2D ID 规则](https://docs.live2d.com/en/cubism-editor-manual/cubism2-handling-of-data/)。

通用壳层与组件来自 LiliaUI；Cubism 连接、领域命令和业务页面由本仓库拥有。

参数工作区按页面编排、状态 composable、领域 utils 和条件异步组件分层；Rust 侧按命令适配、凭据、模型结构、事务原语与会话状态机拆分，保持前后端领域 DTO 和 Tauri 命令稳定。
