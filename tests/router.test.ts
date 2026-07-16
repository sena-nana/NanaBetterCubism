import { fireEvent, render, screen, within } from "@testing-library/vue";
import {
  LiliaAppRoot,
  SETTINGS_CONFIG,
  SETTINGS_TABS,
  normalizeSettingsTab,
  vContextMenu,
} from "@lilia/ui";
import { createMemoryHistory } from "vue-router";
import { describe, expect, it } from "vitest";
import appConfigJson from "../app.config.json";
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
      await screen.findByRole("heading", { level: 1, name: "想在 Cubism Editor 中完成什么？" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "新对话" })).toBeTruthy();
  });

  it("侧边栏提供记忆、设置与状态入口", async () => {
    await renderAt("/");

    expect(screen.getByRole("link", { name: "记忆" })).toBeTruthy();
    expect(screen.getAllByRole("link", { name: "设置" })).toHaveLength(1);
    const primary = document.querySelector('[data-agent-id="sidebar.footer.status"]');
    const model = document.querySelector('[data-agent-id="sidebar.footer.status.model"]');
    const editor = document.querySelector('[data-agent-id="sidebar.footer.status.editor"]');
    expect(primary?.getAttribute("href")).toBe("/settings?tab=model-config");
    expect(model?.closest("a")).toBe(primary);
    expect(editor?.getAttribute("href")).toBe("/settings?tab=editor");
    expect(document.querySelector('[data-agent-id="agent.home.model-settings"]')).toBeNull();
    expect(document.querySelector('[data-agent-id="agent.home.editor-settings"]')).toBeNull();
  });

  it("记忆页可打开", async () => {
    await renderAt("/memory");

    expect(await screen.findByRole("heading", { level: 1, name: "记忆" })).toBeTruthy();
    expect(await screen.findByText("暂无项目阶段记忆")).toBeTruthy();
    expect(await screen.findByText("暂无全局经验")).toBeTruthy();
  });

  it("设置页恢复外观、模型配置、Editor 与关于，并默认显示外观", async () => {
    await renderAt("/settings");

    expect(SETTINGS_CONFIG.defaultTab).toBe("appearance");
    expect(SETTINGS_TABS.map((tab) => tab.key)).toEqual([
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

    expect(normalizeSettingsTab("llm")).toBe("model-config");
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
