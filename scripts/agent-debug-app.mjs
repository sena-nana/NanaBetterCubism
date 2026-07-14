import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const PARAMETER_PAGE = "src/features/parameters/ParameterBatchPage.vue";
const PASTE_PANEL = "src/features/parameters/components/ParameterPastePanel.vue";
const OPERATION_CARD = "src/features/parameters/components/ParameterOperationCard.vue";

const APP_TARGETS = [
  ["parameters.page", PARAMETER_PAGE, "parameters.page"],
  ["parameters.connection.port", PARAMETER_PAGE, "parameters.connection.port"],
  ["parameters.connection.connect", PARAMETER_PAGE, "parameters.connection.connect"],
  ["parameters.rows.add", PARAMETER_PAGE, "parameters.rows.add"],
  ["parameters.rows.open-paste", PARAMETER_PAGE, "parameters.rows.open-paste"],
  ["parameters.paste.input", PASTE_PANEL, "parameters.paste.input"],
  ["parameters.paste.import", PASTE_PANEL, "parameters.paste.import"],
  ["parameters.row.<clientId>", PARAMETER_PAGE, "parameters.row.${row.clientId}"],
  ["parameters.preview.validate", PARAMETER_PAGE, "parameters.preview.validate"],
  ["parameters.preview.execute", PARAMETER_PAGE, "parameters.preview.execute"],
  ["parameters.operation.progress", OPERATION_CARD, "parameters.operation.progress"],
  ["parameters.operation.cancel", OPERATION_CARD, "parameters.operation.cancel"],
];

export function adaptAgentDebugReport(value, projectRoot) {
  const report = structuredClone(value);
  const sourcePath = resolve(projectRoot, PARAMETER_PAGE);
  const source = existsSync(sourcePath) ? readFileSync(sourcePath, "utf-8") : "";
  const template = report.template;

  template.importantFiles = template.importantFiles
    .filter((file) => file.path !== "src/features/home/HomePage.vue")
    .concat([
      fileEntry(PARAMETER_PAGE, "batch parameter workspace", existsSync(sourcePath)),
      fileEntry("src-tauri/src/service.rs", "Cubism Editor session and edit transaction service", existsSync(resolve(projectRoot, "src-tauri/src/service.rs"))),
    ]);
  template.agentTargets = template.agentTargets
    .filter((target) => !target.id.startsWith("home."))
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
