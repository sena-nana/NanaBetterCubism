import { createApp } from "vue";
import {
  createRouter,
  createWebHistory,
  type RouterHistory,
} from "vue-router";
import { installCommandRegistry } from "@lilia/ui/commands";
import { provideLiliaSettings } from "@lilia/ui/settings";
import { LiliaDesktopShell, setLiliaUiConfig } from "@lilia/ui/shell";
import AppRoot from "./AppRoot.vue";
import { appConfig } from "./app.config";
import { commands } from "./commands";
import { installNanaBetterCubismDiagnostics } from "./diagnostics";
import { useLlmConfigStore } from "./features/agent/llmConfigStore";
import { installConversationRuntimeStore } from "./features/agent/conversationRuntimeStore";
import { installAgentShell } from "./features/agent/sidebarConversations";
import { useEditorStore } from "./features/editor/editorStore";
import { routes } from "./routes";
import { installNanaBetterCubismUiRuntime } from "./runtime";
import { settingsModel } from "./settings";

export function createNanaBetterCubismApp(history?: RouterHistory) {
  setLiliaUiConfig(appConfig);
  const app = createApp(AppRoot);
  const router = createNanaBetterCubismRouter(history);

  provideLiliaSettings(app, settingsModel);
  installCommandRegistry(app, commands);
  installNanaBetterCubismUiRuntime(app);
  app.use(router);
  installShellState();
  if (
    import.meta.env.DEV
    && (import.meta.env.VITE_LILIA_AGENT_DEBUG === "1" || import.meta.env.MODE === "agent-debug")
  ) {
    void installNanaBetterCubismDiagnostics();
  }

  return { app, router };
}

export function createNanaBetterCubismRouter(history?: RouterHistory) {
  return createRouter({
    history: history ?? createWebHistory(),
    routes: [
      {
        path: "/",
        component: LiliaDesktopShell,
        meta: { sidebar: "main", returnable: true },
        children: routes,
      },
      { path: "/:pathMatch(.*)*", redirect: "/" },
    ],
  });
}

function installShellState() {
  installAgentShell();
  void installConversationRuntimeStore();
  void useLlmConfigStore().initialize().catch(() => undefined);
  void useEditorStore().initialize();
}
