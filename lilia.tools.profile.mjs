import { defineToolsProfile } from "@lilia/tools";

export default defineToolsProfile({
  requireSingleAppRoot: true,
  expectedDependencies: [
    "@lilia/build",
    "@lilia/config",
    "@lilia/tools",
    "@lilia/ui",
    "vue",
    "vue-router",
  ],
  nativeBackdropPermissions: [
    "lilia:default",
    "lilia:allow-set-window-backdrop",
  ],
  importantFiles: [
    ["app.config.json", "application metadata source"],
    ["src/main.ts", "Vue mount entry"],
    ["src/AppRoot.vue", "application-owned root and global hosts"],
    ["src/app.ts", "Vue, Router, Shell, commands, providers, and business runtime assembly"],
    ["src/app.config.ts", "application shell navigation and live Editor status"],
    ["src/routes.ts", "Cubism Agent routes"],
    ["src/settings.ts", "application settings model"],
    ["src/commands.ts", "application command map"],
    ["src/overlays.ts", "application overlay composition"],
    ["src/runtime.ts", "optional UI runtime installers"],
    ["src/diagnostics.ts", "development-only diagnostics installer"],
    ["src/features/agent/ChatPage.vue", "agent chat workspace"],
    ["src/features/agent/MemoryPage.vue", "memory workspace"],
    ["src/features/agent/components/MemoryListPane.vue", "layered memory navigation"],
    ["src/features/agent/components/MemoryDetailPane.vue", "layered memory content"],
    ["src/features/agent/llmConfigStore.ts", "shared LLM configuration and sidebar status"],
    ["src-tauri/src/agent/mod.rs", "agent runtime module"],
    ["src-tauri/src/service.rs", "Cubism Editor session and edit transaction service"],
    ["tests/router.test.ts", "explicit application assembly and routing tests"],
  ],
  agentTargetFiles: {
    "src/features/agent/ChatHomePage.vue": [
      ["agent.home"],
    ],
    "src/features/agent/components/ConversationComposer.vue": [
      ["agent.home.input", "`${agentIdPrefix}.input`"],
      ["agent.home.send", "`${agentIdPrefix}.send`"],
      ["agent.chat.input", "`${agentIdPrefix}.input`"],
      ["agent.chat.send", "`${agentIdPrefix}.send`"],
      ["agent.chat.ask", "`${agentIdPrefix}.ask`"],
      ["agent.chat.conversation-only", "`${agentIdPrefix}.conversation-only`"],
    ],
    "src/features/agent/components/ConversationSidebarTop.vue": [
      ["sidebar.new-chat"],
      ["sidebar.search.open"],
      ["sidebar.search.input"],
      ["sidebar.search.close"],
      ["sidebar.search.result.<conversationId>", "`sidebar.search.result.${result.id}`"],
    ],
    "src/features/agent/sidebarConversations.ts": [
      ["sidebar.conversation.<conversationId>.pin", "key: \"pin\""],
      ["sidebar.conversation.<conversationId>.delete", "key: \"delete\""],
    ],
    "src/features/agent/ChatPage.vue": [
      ["agent.chat"],
      ["agent.chat.plan"],
    ],
    "src/features/agent/MemoryPage.vue": [
      ["agent.memory"],
    ],
    "src/features/agent/components/MemoryListPane.vue": [
      ["agent.memory.list"],
      ["agent.memory.project-filter"],
      ["agent.memory.search"],
      ["agent.memory.refresh"],
    ],
    "src/features/agent/components/MemoryDetailPane.vue": [
      ["agent.memory.detail"],
      ["agent.memory.detail.empty"],
    ],
    "src/features/agent/settings/LlmSettingsSection.vue": [
      ["settings.llm"],
      ["settings.llm.save"],
    ],
    "src/features/agent/settings/EditorSettingsSection.vue": [
      ["settings.editor"],
    ],
    "src/features/editor/EditorConnectionCard.vue": [
      ["settings.editor.connection.connect", "`${agentIdPrefix}.connect`"],
    ],
  },
  boundaries: {
    includes: [
      "Cubism Editor connection, authorization, and typed edit transactions",
      "Cubism Agent conversations, memory, settings, and application assembly",
      "application-owned Tauri commands and events",
    ],
    excludes: [
      "shared Lilia UI, shell, settings, and runtime implementations",
      "Cubism Core, MOC3 runtime, rendering, and live2d-rs",
      "unverified Editor capabilities such as save, export, animation, physics, and PSD",
    ],
  },
  entrypoints: [
    { id: "dev", command: "yarn dev", purpose: "start the frontend development server" },
    { id: "agent-debug", command: "yarn agent:debug --json", purpose: "inspect application and stable Agent targets" },
    { id: "test", command: "yarn test", purpose: "run frontend behavior and contract tests" },
    { id: "verify", command: "yarn verify", purpose: "run complete frontend and Tauri verification" },
  ],
});
