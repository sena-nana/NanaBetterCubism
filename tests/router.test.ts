import { render, screen } from "@testing-library/vue";
import { APP_SHELL_COPY, LiliaAppRoot, vContextMenu } from "@lilia/ui";
import { createMemoryHistory } from "vue-router";
import { describe, expect, it } from "vitest";
import { createTemplateRouter } from "../src/app";

async function renderAt(path: string) {
  const router = createTemplateRouter(createMemoryHistory());
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

describe("基础路由", () => {
  it("默认首页显示应用首页", async () => {
    await renderAt("/");

    expect(
      await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }),
    ).toBeInTheDocument();
  });

  it("侧边栏左下角提供设置和状态入口", async () => {
    await renderAt("/");

    expect(screen.getAllByRole("link", { name: "设置" })).toHaveLength(1);
    const status = screen.getByRole("link", { name: APP_SHELL_COPY.statusLabel });
    expect(status).toHaveClass("sb-conn--ok");
    expect(status).toHaveAttribute("title", APP_SHELL_COPY.statusTitle);
  });

  it("设置页默认显示外观设置并使用设置侧栏", async () => {
    await renderAt("/settings");

    expect(await screen.findByRole("heading", { level: 1, name: "外观" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "设置分类" })).toBeInTheDocument();
  });

  it("设置页可通过 tab query 显示关于页，未知 tab 回落外观", async () => {
    await renderAt("/settings?tab=about");

    expect(await screen.findByRole("heading", { level: 1, name: "关于" })).toBeInTheDocument();
    expect(await screen.findByText("Tauri Template")).toBeInTheDocument();
  });

  it("未知路由回到首页", async () => {
    await renderAt("/missing");

    expect(await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }))
      .toBeInTheDocument();
  });
});
