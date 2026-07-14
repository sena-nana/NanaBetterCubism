# 开发启动

## 项目结构

```text
Tauri-Template/
├── src/
│   ├── app.config.ts
│   ├── app.ts
│   ├── commands.ts
│   ├── features/
│   ├── routes.ts
│   └── main.ts
├── src-tauri/
└── tests/
```

## 本地运行

```bash
corepack enable
yarn install
yarn dev
yarn tauri:dev
```

## LiliaUI 本地联调

默认 `package.json` 和提交版 `yarn.lock` 固定使用 GitHub 上同一个 LiliaUI commit 的 `@lilia/build`、`@lilia/config`、`@lilia/tools` 和 `@lilia/ui`。普通 `yarn install` 不依赖本机存在 `C:\Files\workspace\LiliaUI`。

需要同时修改 LiliaUI 时，从模板仓库根目录运行：

```bash
yarn liliaui:local
```

该命令会通过 `yarn link --relative` 临时维护项目级 `resolutions`，把四个目标 `@lilia/*` 包切到默认的 `../LiliaUI/packages/*` `portal:` 依赖，并刷新 `node_modules`。如果 LiliaUI 不在相邻目录，可用 `LILIA_UI_LOCAL_PATH` 指定路径：

```powershell
$env:LILIA_UI_LOCAL_PATH = "C:\Files\workspace\LiliaUI"
yarn liliaui:local
Remove-Item Env:LILIA_UI_LOCAL_PATH
```

提交依赖或锁文件变更前，先切回固定 GitHub 依赖：

```bash
yarn liliaui:remote
yarn liliaui:status
```

`yarn liliaui:status` 只检查当前四个 LiliaUI 包来自本地 `portal:` 还是固定 GitHub commit。提交策略是：默认远端 manifest 和锁文件可以入库，本地 `resolutions` / `portal:` lockfile 只作为个人联调状态，不随普通业务提交一起提交。

## 验证

```bash
yarn test
yarn build
cargo check --manifest-path src-tauri/Cargo.toml
yarn verify
```

`yarn agent:debug` 会输出当前脚手架边界、关键文件、Agent 调试环境变量、共享 `data-agent-id` 目标和桌面 replay 工具探测结果。设置 `VITE_LILIA_AGENT_DEBUG=1` 后,基于 `@lilia/ui` 的应用会安装 `window.__liliaAgentDebug` 前端调试接口。
