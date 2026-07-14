import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const CHAT_HOME = "src/features/agent/ChatHomePage.vue";
const CHAT_PAGE = "src/features/agent/ChatPage.vue";
const MEMORY_PAGE = "src/features/agent/MemoryPage.vue";
const LLM_SETTINGS = "src/features/agent/settings/LlmSettingsSection.vue";
const EDITOR_SETTINGS = "src/features/agent/settings/EditorSettingsSection.vue";
const CONNECTION_CARD = "src/features/editor/EditorConnectionCard.vue";

const APP_TARGETS = [
  ["agent.home", CHAT_HOME, "agent.home"],
  ["agent.home.new", CHAT_HOME, "agent.home.new"],
  ["agent.chat", CHAT_PAGE, "agent.chat"],
  ["agent.chat.send", CHAT_PAGE, "agent.chat.send"],
  ["agent.chat.input", CHAT_PAGE, "agent.chat.input"],
  ["agent.chat.ask", CHAT_PAGE, "agent.chat.ask"],
  ["agent.chat.plan", CHAT_PAGE, "agent.chat.plan"],
  ["agent.chat.consolidate", CHAT_PAGE, "agent.chat.consolidate"],
  ["agent.memory", MEMORY_PAGE, "agent.memory"],
  ["settings.llm", LLM_SETTINGS, "settings.llm"],
  ["settings.llm.save", LLM_SETTINGS, "settings.llm.save"],
  ["settings.editor", EDITOR_SETTINGS, "settings.editor"],
  ["settings.editor.connection.connect", CONNECTION_CARD, "`${agentIdPrefix}.connect`"],
];

export function adaptAgentDebugReport(value, projectRoot) {
  const report = structuredClone(value);
  const template = report.template;

  template.importantFiles = template.importantFiles
    .filter((file) => file.path !== "src/features/home/HomePage.vue")
    .concat([
      fileEntry(CHAT_PAGE, "agent chat workspace", existsSync(resolve(projectRoot, CHAT_PAGE))),
      fileEntry(MEMORY_PAGE, "memory workspace", existsSync(resolve(projectRoot, MEMORY_PAGE))),
      fileEntry("src-tauri/src/agent/mod.rs", "agent runtime module", existsSync(resolve(projectRoot, "src-tauri/src/agent/mod.rs"))),
      fileEntry("src-tauri/src/service.rs", "Cubism Editor session and edit transaction service", existsSync(resolve(projectRoot, "src-tauri/src/service.rs"))),
    ]);
  template.agentTargets = template.agentTargets
    .filter((target) => !target.id.startsWith("home.") && !target.id.startsWith("parameters.") && !target.id.startsWith("part-parameters."))
    .concat(APP_TARGETS.map(([id, path, marker]) => {
      const targetPath = resolve(projectRoot, path);
      const targetSource = existsSync(targetPath) ? readFileSync(targetPath, "utf-8") : "";
      return { id, path, exists: targetSource.includes(marker) };
    }));

  updateCheck(template.checks, "important-files-present", template.importantFiles, "files");
  updateCheck(template.checks, "agent-targets-present", template.agentTargets, "targets");
  template.status = template.checks.every((check) => check.ok) ? "ready" : "needs_attention";

  for (const check of report.checks) {
    if (check.id === "template-ready") {
      check.ok = template.status === "ready";
      check.detail = `application status=${template.status}`;
    }
    if (check.id === "agent-targets-ready") {
      check.ok = template.agentTargets.every((target) => target.exists);
      check.detail = `${template.agentTargets.filter((target) => target.exists).length}/${template.agentTargets.length} stable targets present`;
    }
  }
  report.status = report.checks.every((check) => check.ok) ? "ready" : "needs_attention";
  return report;
}

function fileEntry(path, purpose, exists) {
  return { path, purpose, exists };
}

function updateCheck(checks, id, entries, label) {
  const check = checks.find((item) => item.id === id);
  if (!check) return;
  const present = entries.filter((entry) => entry.exists).length;
  check.ok = present === entries.length;
  check.detail = `${present}/${entries.length} ${label} present`;
}
