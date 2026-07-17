import { createApp } from "vue";
import {
  createRouter,
  createWebHistory,
  type RouterHistory,
} from "vue-router";
import { installCommandRegistry } from "@lilia/ui/commands";
import {
  LiliaAppRoot,
  LiliaDesktopShell,
  installLiliaAppRuntime,
} from "@lilia/ui";
import { appConfig } from "./app.config";
import { commands } from "./commands";
import { installNanaBetterCubismDiagnostics } from "./diagnostics";
import { useLlmConfigStore } from "./features/agent/llmConfigStore";
import { installConversationRuntimeStore } from "./features/agent/conversationRuntimeStore";
import { installAgentShell } from "./features/agent/sidebarConversations";
import { useEditorStore } from "./features/editor/editorStore";
import { routes } from "./routes";

export function createNanaBetterCubismApp(history?: RouterHistory) {
  const app = createApp(LiliaAppRoot);
  const router = createNanaBetterCubismRouter(history);

  installLiliaAppRuntime({ app, config: appConfig });
  installCommandRegistry(app, commands);
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
