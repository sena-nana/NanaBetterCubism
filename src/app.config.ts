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
        key: "overview",
        to: "/",
        label: "首页",
        icon: "home",
      },
    ],
    footerLinks: [{ key: "settings", to: "/settings", label: "设置", icon: "settings" }],
    footerStatus: {
      to: "/settings",
      label: appConfigJson.shell.statusLabel,
      title: appConfigJson.shell.statusTitle,
      tone: "ok",
      icon: "sparkles",
    },
  },
} satisfies LiliaAppConfig;
