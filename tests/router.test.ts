import { fireEvent, screen, within } from "@testing-library/vue";
import { normalizeSettingsTab } from "../src/ui";
import { afterEach, describe, expect, it } from "vitest";
import { nextTick, type App } from "vue";
import { createMemoryHistory } from "vue-router";
import appConfigJson from "../app.config.json";
import { createNanaBetterCubismApp } from "../src/app";
import { settingsModel } from "../src/app.config";

const mountedApps: Array<{ app: App; root: HTMLElement }> = [];

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount();
    root.remove();
  }
});

async function renderAt(path: string) {
  const { app, router } = createNanaBetterCubismApp(createMemoryHistory());
  await router.push(path);
  await router.isReady();
  const root = document.createElement("div");
  document.body.append(root);
  app.mount(root);
  mountedApps.push({ app, root });
  await nextTick();
}

describe("Agent 壳层路由", () => {
  it("首页显示 Cubism Agent 入口", async () => {
    await renderAt("/");

    expect(document.querySelector('[data-agent-id="app.provider"]')).toBeTruthy();
    expect(document.querySelector('[data-agent-id="shell"]')).toBeTruthy();
    expect(document.querySelector('[data-agent-id="shell.workspace"]')).toBeTruthy();
    expect(document.querySelector('[data-agent-id="workspace.region.navigation"]')).toBeTruthy();
    expect(document.querySelector('[data-agent-id="workspace.region.primary"]')).toBeTruthy();
    expect(
      await screen.findByRole("heading", { level: 1, name: "想在 Cubism Editor 中完成什么？" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "新对话" })).toBeTruthy();
  });

  it("侧边栏提供记忆、设置与状态入口", async () => {
    await renderAt("/");

    expect(screen.getByRole("link", { name: "记忆" })).toBeTruthy();
    expect(screen.getAllByRole("link", { name: "设置" })).toHaveLength(1);
    expect(document.querySelector('[data-agent-id="sidebar.footer.status.model"]')?.closest("a")?.getAttribute("href")).toBe("/settings?tab=model-config");
    expect(document.querySelector('[data-agent-id="sidebar.footer.status.editor"]')?.closest("a")?.getAttribute("href")).toBe("/settings?tab=editor");
    expect(document.querySelector('[data-agent-id="agent.home.model-settings"]')).toBeNull();
    expect(document.querySelector('[data-agent-id="agent.home.editor-settings"]')).toBeNull();
  });

  it("侧边栏可折叠并使用原有存储键持久化", async () => {
    const storageKey = `${appConfigJson.storageKeyPrefix}.sidebarCollapsed`;
    localStorage.removeItem(storageKey);
    await renderAt("/");

    await fireEvent.click(screen.getByRole("button", { name: "收起侧栏" }));
    expect(localStorage.getItem(storageKey)).toBe("1");
    expect(screen.getByRole("button", { name: "展开侧栏" })).toBeTruthy();
    expect(document.querySelector('[data-agent-id="workspace.region.navigation"]')?.getAttribute("data-region-collapsed")).toBe("true");

    await fireEvent.click(screen.getByRole("button", { name: "展开侧栏" }));
    expect(localStorage.getItem(storageKey)).toBe("0");
    expect(screen.getByRole("button", { name: "收起侧栏" })).toBeTruthy();
  });

  it("记忆页可打开", async () => {
    await renderAt("/memory");

    expect(await screen.findByRole("heading", { level: 1, name: "记忆" })).toBeTruthy();
    expect(await screen.findByText("暂无记忆")).toBeTruthy();
    await fireEvent.click(screen.getByRole("radio", { name: "全局记忆" }));
    expect(await screen.findByText("Agent 保存的跨项目经验会显示在这里。")).toBeTruthy();
  });

  it("设置页恢复外观、模型配置、Editor 与关于，并默认显示外观", async () => {
    await renderAt("/settings");

    expect(document.querySelector('[data-agent-id="settings.sidebar"]')).toBeTruthy();
    expect(screen.getByRole("button", { name: "收起侧栏" }).hasAttribute("disabled")).toBe(true);
    expect(settingsModel.defaultTab).toBe("appearance");
    expect(settingsModel.tabs.map((tab) => tab.key)).toEqual([
      "appearance",
      "model-config",
      "editor",
      "about",
    ]);
    expect(await screen.findByRole("heading", { level: 2, name: "外观" })).toBeTruthy();

    await fireEvent.click(screen.getByRole("radio", { name: "浅色" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem(`${appConfigJson.storageKeyPrefix}.theme`)).toBe("light");
  });

  it("旧 llm tab 映射到模型配置", async () => {
    await renderAt("/settings?tab=llm");

    expect(normalizeSettingsTab(settingsModel, "llm")).toBe("model-config");
    expect(await screen.findByRole("heading", { level: 2, name: "模型配置" })).toBeTruthy();
  });

  it("设置页可通过 tab 显示 Editor", async () => {
    await renderAt("/settings?tab=editor");

    expect(await screen.findByRole("button", { name: "连接 Editor" })).toBeTruthy();
  });

  it("关于页显示 NanaBetterCubism 应用元数据", async () => {
    await renderAt("/settings?tab=about");

    expect(await screen.findByRole("heading", { level: 2, name: "关于" })).toBeTruthy();
    const aboutPage = document.querySelector('[data-agent-id="settings.page.about"]');
    expect(aboutPage).toBeTruthy();
    expect(within(aboutPage as HTMLElement).getByText(appConfigJson.productTitle)).toBeTruthy();
    expect(within(aboutPage as HTMLElement).getByText(appConfigJson.version)).toBeTruthy();
  });

  it("未知路由回到首页", async () => {
    await renderAt("/missing");

    expect(
      await screen.findByRole("heading", { level: 1, name: "想在 Cubism Editor 中完成什么？" }),
    ).toBeTruthy();
  });
});
