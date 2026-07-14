# 桌面应用脚手架

最小 Tauri 2 + Vue 3 + TypeScript 脚手架。通用 UI、配置、工具、构建流程和窗口状态插件由 LiliaUI 提供，本仓库只保留应用配置、路由、命令、业务页面目录和项目专属 Tauri 边界。

## 结构

```text
src/
  main.ts
  app.ts
  app.config.ts
  routes.ts
  commands.ts
  features/
src-tauri/
tests/
```

## 命令

```bash
yarn install
yarn dev
yarn tauri:dev
yarn test
yarn build
yarn verify
```

## 配置

根目录 `app.config.json` 是应用名称、产品标题、版本和 Tauri 标识的同步来源。修改后运行：

```bash
yarn sync:app-config
```

运行时 Shell 配置在 `src/app.config.ts`，路由在 `src/routes.ts`，命令在 `src/commands.ts`。

## 公共依赖

```json
{
  "dependencies": {
    "@lilia/ui": "github:sena-nana/LiliaUI#workspace=@lilia/ui&head=main",
    "@lilia/config": "github:sena-nana/LiliaUI#workspace=@lilia/config&head=main",
    "@lilia/tools": "github:sena-nana/LiliaUI#workspace=@lilia/tools&head=main",
    "@lilia/build": "github:sena-nana/LiliaUI#workspace=@lilia/build&head=main"
  }
}
```

Rust 侧通过 Cargo git dependency 消费 `tauri-plugin-lilia`。
