import { createApp } from "vue";
import { createRouter, createWebHistory, type RouterHistory } from "vue-router";
import { appConfig, settingsModel } from "./app.config";
import { commands } from "./commands";
import { installNanaBetterCubismDiagnostics } from "./diagnostics";
import { installConversationRuntimeStore } from "./features/agent/conversationRuntimeStore";
import { useLlmConfigStore } from "./features/agent/llmConfigStore";
import { installAgentShell } from "./features/agent/sidebarConversations";
import { useEditorStore } from "./features/editor/editorStore";
import AppRoot from "./features/shell/AppRoot.vue";
import { routes } from "./routes";
import {
  installCommandRegistry,
  installCornerStyle,
  installNativeAppearance,
  installTauriNativeAppearanceAdapter,
  provideSettings,
  setLiliaUiConfig,
} from "./ui";

export function createNanaBetterCubismApp(history?: RouterHistory) {
  const app = createApp(AppRoot);
  const router = createNanaBetterCubismRouter(history);

  setLiliaUiConfig(appConfig);
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    installTauriNativeAppearanceAdapter();
  }
  installCornerStyle();
  installNativeAppearance();
  installCommandRegistry(app, commands);
  provideSettings(app, settingsModel);
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
      ...routes,
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
