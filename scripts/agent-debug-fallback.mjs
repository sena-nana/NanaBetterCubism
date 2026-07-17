import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const result = spawnSync(
  process.execPath,
  [
    resolve("node_modules/@lilia/tools/bin/lilia-tools.mjs"),
    "agent-debug",
    ...process.argv.slice(2),
  ],
  { stdio: "inherit" },
);

process.exitCode = result.status ?? 0;
