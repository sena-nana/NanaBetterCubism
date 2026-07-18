import Brain from "@lucide/vue/dist/esm/icons/brain.mjs";
import Info from "@lucide/vue/dist/esm/icons/info.mjs";
import Palette from "@lucide/vue/dist/esm/icons/palette.mjs";
import Server from "@lucide/vue/dist/esm/icons/server.mjs";
import appConfigJson from "../app.config.json";
import {
  LiliaAboutSection,
  LiliaAppearanceSection,
  createSettingsModel,
  type LiliaUiConfig,
} from "./ui";
import ConversationSidebarTop from "./features/agent/components/ConversationSidebarTop.vue";
import { editorFooterStatus, modelFooterStatus } from "./features/shell/footerSelfCheck";

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
      { key: "home", to: "/", label: "首页", icon: "home" },
      { key: "memory", to: "/memory", label: "记忆", icon: "brain" },
    ],
    groups: [
      { key: "conversations", title: appConfigJson.shell.workspaceSectionTitle, emptyText: "暂无对话", items: [] },
    ],
    footerLinks: [
      { key: "settings", to: "/settings", label: "设置", icon: "settings" },
    ],
    footerStatuses: [
      { key: "model", ...modelFooterStatus },
      { key: "editor", ...editorFooterStatus },
    ],
  },
} satisfies LiliaUiConfig;

export const settingsModel = createSettingsModel({
  path: "/settings",
  aliases: { llm: "model-config" },
  defaultTab: "appearance",
  hideHeader: true,
  tabs: [
    { key: "appearance", label: "外观", icon: Palette },
    { key: "model-config", label: "模型配置", icon: Brain },
    { key: "editor", label: "Editor", icon: Server },
    { key: "about", label: "关于", icon: Info },
  ],
  sections: {
    appearance: LiliaAppearanceSection,
    "model-config": () => import("./features/agent/settings/LlmSettingsSection.vue"),
    editor: () => import("./features/agent/settings/EditorSettingsSection.vue"),
    about: LiliaAboutSection,
  },
});
