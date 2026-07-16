import appConfigJson from "../app.config.json";
import type { LiliaAppConfig } from "@lilia/ui";
import ConversationSidebarTop from "./features/agent/components/ConversationSidebarTop.vue";

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
        title: appConfigJson.shell.statusTitle,
        tone: "warn",
        icon: "server",
      },
    ],
  },
  settings: {
    aliases: {
      llm: "model-config",
    },
    defaultTab: "appearance",
    hideHeader: true,
    tabs: [
      { key: "appearance", label: "外观", icon: "palette" },
      { key: "model-config", label: "模型配置", icon: "brain" },
      { key: "editor", label: "Editor", icon: "server" },
      { key: "about", label: "关于", icon: "info" },
    ],
    sections: {
      appearance: () => import("@lilia/ui/pages/settings/AppearanceSection"),
      "model-config": () => import("./features/agent/settings/LlmSettingsSection.vue"),
      editor: () => import("./features/agent/settings/EditorSettingsSection.vue"),
      about: () => import("@lilia/ui/pages/settings/AboutSection"),
    },
  },
} satisfies LiliaAppConfig;
