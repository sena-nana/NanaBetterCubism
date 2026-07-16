import appConfigJson from "../app.config.json";
import type { LiliaUiConfig } from "@lilia/ui/shell";
import ConversationSidebarTop from "./features/agent/components/ConversationSidebarTop.vue";

export const appConfig = {
  appName: appConfigJson.appName,
  productTitle: appConfigJson.productTitle,
  version: appConfigJson.version,
  storageKeyPrefix: appConfigJson.storageKeyPrefix,
  appearance: {
    backdropTarget: "sidebar",
  },
  sidebar: {
    topContent: ConversationSidebarTop,
    nav: [
      {
        key: "memory",
        to: "/memory",
        label: "记忆",
        icon: "brain",
      },
    ],
    groups: [
      {
        key: "conversations",
        title: "对话",
        emptyText: "暂无对话",
        items: [],
      },
    ],
    footerLinks: [{ key: "settings", to: "/settings", label: "设置", icon: "settings" }],
    footerStatuses: [
      {
        key: "model",
        to: "/settings?tab=model-config",
        label: "模型读取中",
        title: "正在读取模型配置。",
        tone: "warn",
        icon: "brain",
      },
      {
        key: "editor",
        to: "/settings?tab=editor",
        label: "Editor 未连接",
        title: "在设置中连接 Cubism Editor。",
        tone: "warn",
        icon: "server",
      },
    ],
  },
} satisfies LiliaUiConfig;
