import { createLiliaApp, createLiliaRouter, LiliaDesktopShell, setLiliaAppConfig } from "@lilia/ui";
import type { RouterHistory } from "vue-router";
import { appConfig } from "./app.config";
import { commands } from "./commands";
import { useLlmConfigStore } from "./features/agent/llmConfigStore";
import { installConversationRuntimeStore } from "./features/agent/conversationRuntimeStore";
import { installAgentShell } from "./features/agent/sidebarConversations";
import { useEditorStore } from "./features/editor/editorStore";
import { routes } from "./routes";

export function createNanaBetterCubismApp(history?: RouterHistory) {
  const created = createLiliaApp({
    config: appConfig,
    routes,
    commands,
    shell: LiliaDesktopShell,
    history,
  });
  installShellState();
  return created;
}

export function createNanaBetterCubismRouter(history?: RouterHistory) {
  setLiliaAppConfig(appConfig);
  const router = createLiliaRouter(routes, LiliaDesktopShell, history);
  installShellState();
  return router;
}

function installShellState() {
  installAgentShell();
  void installConversationRuntimeStore();
  void useLlmConfigStore().initialize().catch(() => undefined);
  void useEditorStore().initialize();
}
