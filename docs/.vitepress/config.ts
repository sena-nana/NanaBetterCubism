import { defineLiliaDocsConfig } from "@lilia/config";

export default defineLiliaDocsConfig({
  title: "Tauri Template",
  description: "A minimal Tauri 2 + Vue 3 desktop application template.",
  nav: [{ text: "开发启动", link: "/guide/development" }],
  sidebar: [
    {
      text: "指南",
      items: [{ text: "开发启动", link: "/guide/development" }],
    },
  ],
});
