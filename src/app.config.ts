import appConfigJson from "../app.config.json";
import type { LiliaAppConfig } from "@lilia/ui";

export const appConfig = {
  appName: appConfigJson.appName,
  productTitle: appConfigJson.productTitle,
  version: appConfigJson.version,
  storageKeyPrefix: appConfigJson.storageKeyPrefix,
  appearance: {
    backdropTarget: "sidebar",
  },
  shell: appConfigJson.shell,
  sidebar: {
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
    globalActions: [
      {
        key: "new-chat",
        label: "新对话",
        icon: "file-plus",
      },
    ],
    footerLinks: [{ key: "settings", to: "/settings", label: "设置", icon: "settings" }],
    footerStatus: {
      to: "/settings?tab=editor",
      label: appConfigJson.shell.statusLabel,
      title: appConfigJson.shell.statusTitle,
      tone: "warn",
      icon: "server",
    },
  },
  settings: {
    defaultTab: "llm",
    hideHeader: true,
    tabs: [
      { key: "llm", label: "模型", icon: "sparkles" },
      { key: "editor", label: "Editor", icon: "server" },
      { key: "appearance", label: "外观", icon: "palette" },
      { key: "about", label: "关于", icon: "info" },
    ],
    sections: {
      llm: () => import("./features/agent/settings/LlmSettingsSection.vue"),
      editor: () => import("./features/agent/settings/EditorSettingsSection.vue"),
    },
  },
} satisfies LiliaAppConfig;
