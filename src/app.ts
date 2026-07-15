import { createLiliaApp, createLiliaRouter, LiliaDesktopShell, setLiliaAppConfig } from "@lilia/ui";
import type { RouterHistory } from "vue-router";
import { appConfig } from "./app.config";
import { commands } from "./commands";
import { installConversationRuntimeStore } from "./features/agent/conversationRuntimeStore";
import { installAgentShell } from "./features/agent/sidebarConversations";
import { routes } from "./routes";

export function createNanaBetterCubismApp(history?: RouterHistory) {
  const created = createLiliaApp({
    config: appConfig,
    routes,
    commands,
    shell: LiliaDesktopShell,
    history,
  });
  installAgentShell();
  void installConversationRuntimeStore();
  return created;
}

export function createNanaBetterCubismRouter(history?: RouterHistory) {
  setLiliaAppConfig(appConfig);
  const router = createLiliaRouter(routes, LiliaDesktopShell, history);
  installAgentShell();
  void installConversationRuntimeStore();
  return router;
}
