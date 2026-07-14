import { render, screen } from "@testing-library/vue";
import { APP_SHELL_COPY, LiliaAppRoot, SETTINGS_TABS, vContextMenu } from "@lilia/ui";
import { createMemoryHistory } from "vue-router";
import { describe, expect, it } from "vitest";
import { createNanaBetterCubismRouter } from "../src/app";

async function renderAt(path: string) {
  const router = createNanaBetterCubismRouter(createMemoryHistory());
  await router.push(path);
  await router.isReady();

  render(LiliaAppRoot, {
    global: {
      directives: {
        contextMenu: vContextMenu,
      },
      plugins: [router],
    },
  });
}

describe("Agent 壳层路由", () => {
  it("首页显示 Cubism Agent 入口", async () => {
    await renderAt("/");

    expect(
      await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "新对话" })).toBeTruthy();
  });

  it("侧边栏提供记忆、设置与状态入口", async () => {
    await renderAt("/");

    expect(screen.getByRole("link", { name: "记忆" })).toBeTruthy();
    expect(screen.getAllByRole("link", { name: "设置" })).toHaveLength(1);
    const status = document.querySelector('[data-agent-id="sidebar.footer.status"]');
    expect(status).toBeTruthy();
    expect(status?.className).toContain("sb-conn--warn");
    expect(status?.getAttribute("href")).toContain("/settings");
  });

  it("记忆页可打开", async () => {
    await renderAt("/memory");

    expect(await screen.findByRole("heading", { level: 1, name: "记忆" })).toBeTruthy();
    expect(await screen.findByText("暂无项目阶段记忆")).toBeTruthy();
    expect(await screen.findByText("暂无全局经验")).toBeTruthy();
  });

  it("设置页包含模型与 Editor tab", async () => {
    await renderAt("/settings");

    expect(SETTINGS_TABS.some((tab) => tab.key === "llm")).toBe(true);
    expect(SETTINGS_TABS.some((tab) => tab.key === "editor")).toBe(true);
    expect(await screen.findByRole("heading", { level: 1, name: "模型" })).toBeTruthy();
  });

  it("设置页可通过 tab 显示 Editor", async () => {
    await renderAt("/settings?tab=editor");

    expect(await screen.findByRole("heading", { level: 1, name: "Editor" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "连接 Editor" })).toBeTruthy();
  });

  it("未知路由回到首页", async () => {
    await renderAt("/missing");

    expect(
      await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }),
    ).toBeTruthy();
  });
});
