import { defineToolsProfile } from "@lilia/tools";

export default defineToolsProfile({
  requireSingleAppRoot: true,
  expectedDependencies: [
    "@lilia/build",
    "@lilia/config",
    "@lilia/tools",
    "@lilia/ui-contract",
    "@lilia/ui-foundation",
    "@lilia/ui",
    "vue",
    "vue-router",
  ],
  nativeBackdropPermissions: ["lilia:default", "lilia:allow-set-window-backdrop"],
  importantFiles: [
    ["app.config.json", "application metadata source"],
    ["src/main.ts", "Vue mount entry"],
    ["src/app.ts", "Vue, Router, commands, settings, and runtime assembly"],
    ["src/features/shell/AppRoot.vue", "Provider, shell, workspace, and route composition"],
    ["src/routes.ts", "application routes"],
    ["src/ui/index.ts", "active UI facade"],
    ["src/ui/preset.ts", "active preset adapter"],
    ["tests/router.test.ts", "application shell and routing behavior"],
    ["tests/tooling.test.ts", "tooling contract tests"],
  ],
  agentTargetFiles: {
    "src/features/home/HomePage.vue": [["agent.home"]],
    "src/features/agent/components/ConversationSidebarTop.vue": [["sidebar.new-chat"], ["sidebar.search.open"]],
    "src/features/shell/AppRoot.vue": [["shell.sidebar.toggle"]],
  },
  boundaries: {
    includes: ["Cubism Editor integration", "agent, memory, settings, and application shell composition"],
    excludes: ["shared Lilia UI implementation", "shared build and Tauri backdrop runtime"],
  },
  entrypoints: [
    { id: "dev", command: "yarn dev", purpose: "start the frontend development server" },
    { id: "agent-debug", command: "yarn agent:debug --json", purpose: "inspect Agent Debug readiness" },
    { id: "test", command: "yarn test", purpose: "run application behavior tests" },
    { id: "verify", command: "yarn verify", purpose: "run complete application verification" },
  ],
});
