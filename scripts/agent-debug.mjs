import { runToolsCli } from "@lilia/tools";

runToolsCli(["agent-debug", ...process.argv.slice(2)], {
  env: process.env,
  projectRoot: process.cwd(),
});
