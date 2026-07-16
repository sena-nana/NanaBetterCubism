import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import MemoryPage from "../src/features/agent/MemoryPage.vue";
import type { MemoryRecord, MemoryScope } from "../src/features/agent/types";

const projectMemories: MemoryRecord[] = [
  {
    id: "project-memory-a",
    scope: "project",
    projectId: "project-a",
    projectName: "角色 A",
    title: "眼睛参数",
    layers: [
      { name: "Overview", content: "已完成眼睛参数结构。" },
      { name: "Stage", content: "ParamAngleX 已对齐。" },
      { name: "Structure", content: "" },
      { name: "Decisions", content: "保留标准参数 ID。" },
    ],
    enabled: true,
    sourceConversationId: "conversation-a",
    updatedAt: "2026-07-16T00:00:00Z",
  },
  {
    id: "project-memory-b",
    scope: "project",
    projectId: "project-b",
    projectName: "角色 B",
    title: "嘴部结构",
    layers: [
      { name: "Overview", content: "已建立嘴部层级。" },
      { name: "Stage", content: "" },
      { name: "Structure", content: "口腔位于嘴形之下。" },
      { name: "Decisions", content: "" },
    ],
    enabled: false,
    sourceConversationId: null,
    updatedAt: "2026-07-15T00:00:00Z",
  },
];

const globalMemories: MemoryRecord[] = [
  {
    id: "global-memory",
    scope: "global",
    projectId: null,
    projectName: null,
    title: "参数命名经验",
    layers: [
      { name: "Summary", content: "先核对已有参数 ID。" },
      { name: "Technique", content: "列出参数后再创建。" },
      { name: "Caveats", content: "" },
    ],
    enabled: true,
    sourceConversationId: null,
    updatedAt: "2026-07-16T00:00:00Z",
  },
];

const bridge = vi.hoisted(() => ({
  listMemories: vi.fn(),
  listProjects: vi.fn(),
  normalizeCommandError: vi.fn((error: unknown) => ({
    message: error instanceof Error ? error.message : String(error),
  })),
  setMemoryEnabled: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

beforeEach(() => {
  bridge.listProjects.mockReset().mockResolvedValue([
    { id: "project-a", name: "角色 A", updatedAt: "2026-07-16T00:00:00Z" },
    { id: "project-b", name: "角色 B", updatedAt: "2026-07-15T00:00:00Z" },
  ]);
  bridge.listMemories.mockReset().mockImplementation(
    async (scope: MemoryScope, projectId: string | null) => {
      if (scope === "global") return globalMemories;
      return projectId
        ? projectMemories.filter((memory) => memory.projectId === projectId)
        : projectMemories;
    },
  );
  bridge.setMemoryEnabled.mockReset().mockResolvedValue(undefined);
  bridge.normalizeCommandError.mockClear();
});

async function renderPage(path = "/memory") {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/memory", component: MemoryPage },
      { path: "/chats/:id", component: { template: "<div />" } },
    ],
  });
  await router.push(path);
  await router.isReady();
  const view = render({ template: "<RouterView />" }, { global: { plugins: [router] } });
  return { ...view, router };
}

describe("分层记忆浏览页", () => {
  it("按范围和项目浏览记忆，并以固定顺序显示各层", async () => {
    const { container, router } = await renderPage();

    expect(await screen.findByRole("heading", { name: "眼睛参数" })).toBeTruthy();
    expect(bridge.listMemories).toHaveBeenCalledWith("project", null);
    expect(screen.getByText("ParamAngleX 已对齐。")).toBeTruthy();
    expect(screen.getByText("此层暂无内容。")).toBeTruthy();
    expect(screen.getByRole("link", { name: "查看来源对话" }).getAttribute("href"))
      .toBe("/chats/conversation-a");

    const layerNames = Array.from(container.querySelectorAll("[data-layer]"))
      .map((element) => element.getAttribute("data-layer"));
    expect(layerNames).toEqual(["Overview", "Stage", "Structure", "Decisions"]);

    await fireEvent.click(screen.getByRole("radio", { name: "全局记忆" }));
    expect(await screen.findByRole("heading", { name: "参数命名经验" })).toBeTruthy();
    expect(bridge.listMemories).toHaveBeenLastCalledWith("global", null);
    expect(Array.from(container.querySelectorAll("[data-layer]")).map((element) =>
      element.getAttribute("data-layer")))
      .toEqual(["Summary", "Technique", "Caveats"]);
    expect(router.currentRoute.value.query.scope).toBe("global");

    await fireEvent.click(screen.getByRole("radio", { name: "项目记忆" }));
    await screen.findByRole("heading", { name: "眼睛参数" });
    await fireEvent.update(screen.getByRole("combobox"), "project-b");
    expect(await screen.findByRole("heading", { name: "嘴部结构" })).toBeTruthy();
    expect(bridge.listMemories).toHaveBeenLastCalledWith("project", "project-b");
    expect(router.currentRoute.value.query.project).toBe("project-b");
  });

  it("搜索标题、项目和分层正文，并可重新启用停用记忆", async () => {
    await renderPage();
    await screen.findByRole("heading", { name: "眼睛参数" });

    await fireEvent.update(screen.getByRole("searchbox", { name: "搜索记忆" }), "口腔位于");
    expect(await screen.findByRole("heading", { name: "嘴部结构" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "眼睛参数" })).toBeNull();

    const toggle = screen.getByRole("switch", { name: "启用记忆" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    await fireEvent.click(toggle);
    expect(bridge.setMemoryEnabled).toHaveBeenCalledWith("project-memory-b", true);
    expect(bridge.listMemories).toHaveBeenCalledTimes(2);
  });

  it("在读取和启停失败时保留真实状态并提供恢复入口", async () => {
    bridge.listMemories.mockRejectedValueOnce(new Error("记忆读取失败"));
    await renderPage();

    expect(await screen.findByRole("alert")).toBeTruthy();
    bridge.listMemories.mockImplementation(async () => projectMemories);
    await fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("heading", { name: "眼睛参数" })).toBeTruthy();

    bridge.setMemoryEnabled.mockRejectedValueOnce(new Error("启停操作失败"));
    const toggle = screen.getByRole("switch", { name: "启用记忆" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    await fireEvent.click(toggle);
    await waitFor(() => expect(screen.getByText("启停操作失败")).toBeTruthy());
    expect(toggle.getAttribute("aria-checked")).toBe("true");
  });

  it("项目列表读取失败后结束加载并可重试", async () => {
    bridge.listProjects.mockRejectedValueOnce(new Error("项目读取失败"));
    await renderPage();

    expect(await screen.findByText("项目读取失败")).toBeTruthy();
    expect(screen.getByRole("button", { name: "重试" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("heading", { name: "眼睛参数" })).toBeTruthy();
  });

  it("忽略切换范围前尚未完成的旧请求", async () => {
    let resolveProject!: (value: MemoryRecord[]) => void;
    bridge.listMemories
      .mockImplementationOnce(
        () => new Promise<MemoryRecord[]>((resolve) => {
          resolveProject = resolve;
        }),
      )
      .mockResolvedValueOnce(globalMemories);
    await renderPage();
    await waitFor(() => expect(bridge.listMemories).toHaveBeenCalledTimes(1));

    await fireEvent.click(screen.getByRole("radio", { name: "全局记忆" }));
    expect(await screen.findByRole("heading", { name: "参数命名经验" })).toBeTruthy();
    resolveProject(projectMemories);
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "参数命名经验" })).toBeTruthy();
      expect(screen.queryByRole("heading", { name: "眼睛参数" })).toBeNull();
    });
  });
});
