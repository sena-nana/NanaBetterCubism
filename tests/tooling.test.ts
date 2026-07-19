import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

function appConfig() {
  return JSON.parse(readFileSync(resolve("app.config.json"), "utf-8")) as {
    appName: string;
    productTitle: string;
    version: string;
    identifier: string;
  };
}

function yarnRun(args: string[], options: Parameters<typeof spawnSync>[2]) {
  if (process.platform !== "win32") {
    return spawnSync("yarn", args, options);
  }

  return spawnSync(process.env.ComSpec || "cmd.exe", ["/d", "/s", "/c", "yarn.cmd", ...args], options);
}

describe("单应用模板工具链", () => {
  it("Agent 调试入口输出模板边界和可执行验证入口", () => {
    const run = yarnRun(["agent:debug", "--json"], {
      cwd: resolve("."),
      encoding: "utf-8",
    });

    expect(run.status).toBe(0);

    const report = JSON.parse(run.stdout) as {
      mode: string;
      status: string;
      desktopReplay: { requiredForReadiness: boolean };
      environment: { frontendFlag: string };
      checks: Array<{ id: string; ok: boolean }>;
      template: {
        entrypoints: Array<{ id: string; command: string }>;
        importantFiles: Array<{ path: string; exists: boolean }>;
        agentTargets: Array<{ id: string; path: string; exists: boolean }>;
      };
    };

    expect(report.mode).toBe("agent-debug-readiness");
    expect(report.status).toBe("ready");
    expect(report.environment.frontendFlag).toBe("VITE_LILIA_AGENT_DEBUG=1");
    expect(report.desktopReplay.requiredForReadiness).toBe(false);
    expect(report.template.entrypoints.length).toBeGreaterThan(0);
    expect(report.template.importantFiles.every((file) => file.exists)).toBe(true);
    expect(report.template.agentTargets.every((target) => target.exists)).toBe(true);
    expect(report.checks.every((check) => check.ok)).toBe(true);
  }, 15_000);

  it("同步应用元数据并保留主窗口透明合成层", () => {
    const config = appConfig();
    const pkg = JSON.parse(readFileSync(resolve("package.json"), "utf-8"));
    const tauri = JSON.parse(readFileSync(resolve("src-tauri/tauri.conf.json"), "utf-8"));

    expect(pkg.name).toBe(config.appName);
    expect(pkg.version).toBe(config.version);
    expect(tauri.productName).toBe(config.productTitle);
    expect(tauri.version).toBe(config.version);
    expect(tauri.identifier).toBe(config.identifier);
    expect(tauri.app.windows[0].title).toBe(config.productTitle);
    expect(tauri.app.windows[0].transparent).toBe(true);
    expect(tauri.app.windows[0].backgroundColor).toBe("#00000000");
  });
});
