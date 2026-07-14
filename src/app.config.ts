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
        key: "parameters",
        to: "/",
        label: "批量参数",
        icon: "workflow",
      },
    ],
    footerLinks: [{ key: "settings", to: "/settings", label: "设置", icon: "settings" }],
    footerStatus: {
      to: "/",
      label: appConfigJson.shell.statusLabel,
      title: appConfigJson.shell.statusTitle,
      tone: "warn",
      icon: "server",
    },
  },
} satisfies LiliaAppConfig;
