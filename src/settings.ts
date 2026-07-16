import {
  createLiliaSettingsModel,
  LiliaAboutSection,
  LiliaAppearanceSection,
} from "@lilia/ui/settings";
import { resolveLiliaIcon } from "@lilia/ui/shell";

export const settingsModel = createLiliaSettingsModel({
  path: "/settings",
  aliases: {
    llm: "model-config",
  },
  defaultTab: "appearance",
  description: "模型、Editor 与外观偏好会保存在本地。",
  hideHeader: true,
  tabs: [
    { key: "appearance", label: "外观", icon: resolveLiliaIcon("palette") },
    { key: "model-config", label: "模型配置", icon: resolveLiliaIcon("brain") },
    { key: "editor", label: "Editor", icon: resolveLiliaIcon("server") },
    { key: "about", label: "关于", icon: resolveLiliaIcon("info") },
  ],
  sections: {
    appearance: LiliaAppearanceSection,
    "model-config": () => import("./features/agent/settings/LlmSettingsSection.vue"),
    editor: () => import("./features/agent/settings/EditorSettingsSection.vue"),
    about: LiliaAboutSection,
  },
});
