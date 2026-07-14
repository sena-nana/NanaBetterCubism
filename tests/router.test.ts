import { fireEvent, render, screen } from "@testing-library/vue";
import { APP_SHELL_COPY, LiliaAppRoot, vContextMenu } from "@lilia/ui";
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

describe("基础路由", () => {
  it("默认首页显示应用首页", async () => {
    await renderAt("/");

    expect(
      await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }),
    ).toBeInTheDocument();
  });

  it("支持粘贴带表头 TSV 并即时生成统一 ID", async () => {
    await renderAt("/");

    await fireEvent.click(screen.getByRole("button", { name: "粘贴批量数据" }));
    const input = await screen.findByPlaceholderText(/前发摆动/);
    await fireEvent.update(input, "名称\tID 段\t方位\n左眼\tEye\tL\n右眼\tEye\tR");
    await fireEvent.click(screen.getByRole("button", { name: "导入" }));

    expect((await screen.findAllByText("ParamEyeL01")).length).toBeGreaterThan(0);
    expect((await screen.findAllByText("ParamEyeR02")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "校验当前模型" })).toBeDisabled();
  });

  it("侧边栏左下角提供设置和状态入口", async () => {
    await renderAt("/");

    expect(screen.getAllByRole("link", { name: "设置" })).toHaveLength(1);
    const status = screen.getByRole("link", { name: APP_SHELL_COPY.statusLabel });
    expect(status).toHaveClass("sb-conn--warn");
    expect(status).toHaveAttribute("title", APP_SHELL_COPY.statusTitle);
  });

  it("提供独立的部件参数查询页面和侧边栏入口", async () => {
    await renderAt("/part-parameters");

    expect(await screen.findByRole("heading", { level: 1, name: "部件关联参数" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "部件参数" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查找关联参数" })).toBeDisabled();
    expect(screen.getByText("尚未查询")).toBeInTheDocument();
  });

  it("设置页默认显示外观设置并使用设置侧栏", async () => {
    await renderAt("/settings");

    expect(await screen.findByRole("heading", { level: 1, name: "外观" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "设置分类" })).toBeInTheDocument();
  });

  it("设置页可通过 tab query 显示关于页，未知 tab 回落外观", async () => {
    await renderAt("/settings?tab=about");

    expect(await screen.findByRole("heading", { level: 1, name: "关于" })).toBeInTheDocument();
    expect((await screen.findAllByText("NanaBetterCubism")).length).toBeGreaterThan(0);
  });

  it("未知路由回到首页", async () => {
    await renderAt("/missing");

    expect(await screen.findByRole("heading", { level: 1, name: APP_SHELL_COPY.homeTitle }))
      .toBeInTheDocument();
  });
});
