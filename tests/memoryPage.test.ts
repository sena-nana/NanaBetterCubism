import { fireEvent, render, screen } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { describe, expect, it, vi } from "vitest";
import MemoryPage from "../src/features/agent/MemoryPage.vue";

const bridge = vi.hoisted(() => ({
  listMemories: vi.fn(async () => [
    {
      id: "project-memory",
      scope: "project" as const,
      kind: "stage" as const,
      projectId: "project-a",
      projectName: "角色 A",
      title: "眼睛参数",
      body: "# 眼睛参数\n\n## Overview\n已完成眼睛参数结构。\n\n## Stage\nParamAngleX 已对齐。\n\n## Structure\n\n## Decisions\n",
      enabled: true,
      sourceConversationId: "conversation-a",
      updatedAt: "2026-07-16T00:00:00Z",
    },
    {
      id: "global-memory",
      scope: "global" as const,
      kind: "experience" as const,
      projectId: null,
      projectName: null,
      title: "参数命名经验",
      body: "# 参数命名经验\n\n## Summary\n先核对已有参数 ID。\n\n## Technique\n\n## Caveats\n",
      enabled: true,
      sourceConversationId: null,
      updatedAt: "2026-07-16T00:00:00Z",
    },
  ]),
  listProjects: vi.fn(async () => [
    { id: "project-a", name: "角色 A", updatedAt: "2026-07-16T00:00:00Z" },
  ]),
  normalizeCommandError: vi.fn((error: unknown) => ({ message: String(error) })),
  setMemoryEnabled: vi.fn(async () => undefined),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

describe("记忆管理页", () => {
  it("优先展示 Overview/Summary，并允许停用", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/memory", component: MemoryPage }],
    });
    await router.push("/memory");
    await router.isReady();
    render({ template: "<RouterView />" }, { global: { plugins: [router] } });

    expect(await screen.findAllByText("眼睛参数")).toHaveLength(2);
    expect(screen.getAllByText("已完成眼睛参数结构。").length).toBeGreaterThan(0);
    expect(screen.getAllByText("先核对已有参数 ID。").length).toBeGreaterThan(0);
    expect(screen.getAllByText("分层正文")).toHaveLength(2);
    expect(bridge.listMemories).toHaveBeenCalledWith(null);

    await fireEvent.click(screen.getAllByRole("switch", { name: "启用" })[0]);
    expect(bridge.setMemoryEnabled).toHaveBeenCalledWith("project-memory", false);
  });
});
