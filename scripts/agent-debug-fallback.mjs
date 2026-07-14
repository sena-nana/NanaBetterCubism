import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export function createFallbackAgentDebugReport(tools, projectRoot) {
  const template = tools.createTemplateReport(projectRoot);
  const appConfig = readJson(resolve(projectRoot, "app.config.json"));
  const packageJson = readJson(resolve(projectRoot, "package.json"));
  const envPrefix = toEnvPrefix(appConfig.appName);
  const runtimeTools = [
    runtimeTool("node", ["--version"], "runs shared Lilia tooling"),
    runtimeTool("cargo", ["--version"], "installs tauri-driver when desktop UI replay is needed"),
    runtimeTool("tauri-driver", ["--help"], "bridges WebDriver actions to a Tauri desktop app"),
    runtimeTool("msedgedriver", ["--version"], "drives Microsoft Edge for WebDriver-based desktop checks"),
  ];
  const hasWebDriver = runtimeTools.some((tool) => tool.id === "msedgedriver" && tool.available);
  const checks = createChecks(template);

  return {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    mode: "agent-debug-readiness",
    project: template.project,
    status: checks.every((check) => check.ok) ? "ready" : "needs_attention",
    scripts: {
      agentDebug: packageJson.scripts?.["agent:debug"] ?? null,
      verifyAgentDebug: packageJson.scripts?.["verify:agent-debug"] ?? null,
    },
    environment: {
      frontendFlag: "VITE_LILIA_AGENT_DEBUG=1",
      backendFlag: "LILIA_AGENT_DEBUG=1",
      devPortVariable: `${envPrefix}_DEV_PORT`,
      strictPortVariable: `${envPrefix}_DEV_STRICT_PORT`,
    },
    runtimeTools,
    desktopReplay: {
      available: runtimeTools.some((tool) => tool.id === "tauri-driver" && tool.available) && hasWebDriver,
      requiredForReadiness: false,
      note: "Desktop replay uses external tools when a final app defines real agent-debug scenarios.",
    },
    checks,
    template,
  };
}

export function printFallbackAgentDebugReport(value) {
  console.log(`${value.project.productTitle} agent debug readiness`);
  console.log(`status: ${value.status}`);
  console.log("");
  console.log("checks:");
  for (const check of value.checks) {
    console.log(`- ${check.ok ? "ok" : "fail"} ${check.id}: ${check.detail}`);
  }
  console.log("");
  console.log("runtime tools:");
  for (const tool of value.runtimeTools) {
    console.log(`- ${tool.available ? "ok" : "missing"} ${tool.id}: ${tool.purpose}`);
  }
}

function createChecks(template) {
  return [
    {
      id: "template-ready",
      ok: template.status === "ready",
      detail: `template status=${template.status}`,
    },
    {
      id: "agent-targets-ready",
      ok: template.agentTargets.every((target) => target.exists),
      detail: `${template.agentTargets.filter((target) => target.exists).length}/${template.agentTargets.length} stable targets present`,
    },
  ];
}

function runtimeTool(command, toolArgs, purpose) {
  const result = spawnSync(command, toolArgs, {
    encoding: "utf-8",
    shell: false,
    stdio: "pipe",
  });
  return {
    id: command,
    available: result.error === undefined && result.status === 0,
    purpose,
    detail: result.error?.message ?? firstLine(result.stdout || result.stderr),
  };
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf-8"));
}

function firstLine(value = "") {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? "";
}

function toEnvPrefix(appName) {
  return appName.replace(/[^a-zA-Z0-9]+/g, "_").replace(/^_+|_+$/g, "").toUpperCase();
}
