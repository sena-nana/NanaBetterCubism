#!/usr/bin/env node

import { createFallbackAgentDebugReport, printFallbackAgentDebugReport } from "./agent-debug-fallback.mjs";

const args = process.argv.slice(2);
const projectRoot = process.cwd();
const tools = await import("@lilia/tools");
const report = typeof tools.createAgentDebugReport === "function"
  ? tools.createAgentDebugReport(projectRoot)
  : createFallbackAgentDebugReport(tools, projectRoot);

if (args.includes("--json")) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else if (typeof tools.printAgentDebugReport === "function") {
  tools.printAgentDebugReport(report);
} else {
  printFallbackAgentDebugReport(report);
}

if (report.status !== "ready") {
  process.exitCode = 1;
}
