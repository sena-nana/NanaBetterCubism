import appConfigJson from "../app.config.json";
import { LiliaAppearanceSection, type LiliaAppConfig } from "@lilia/ui";
import LiliaAboutSection from "@lilia/ui/pages/settings/AboutSection";
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
        key: "selfcheck",
        to: "/settings?tab=model-config",
        label: "模型读取中",
        title: "正在读取模型配置。",
        tone: "warn",
        icon: "brain",
      },
    ],
  },
  settings: {
    path: "/settings",
    aliases: { llm: "model-config" },
    defaultTab: "appearance",
    hideHeader: true,
    tabs: [
      { key: "appearance", label: "外观", icon: "palette" },
      { key: "model-config", label: "模型配置", icon: "brain" },
      { key: "editor", label: "Editor", icon: "server" },
      { key: "about", label: "关于", icon: "info" },
    ],
    sections: {
      appearance: LiliaAppearanceSection,
      "model-config": () => import("./features/agent/settings/LlmSettingsSection.vue"),
      editor: () => import("./features/agent/settings/EditorSettingsSection.vue"),
      about: LiliaAboutSection,
    },
  },
} satisfies LiliaAppConfig;
