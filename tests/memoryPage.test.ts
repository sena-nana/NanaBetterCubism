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
      body: "已完成眼睛参数结构。",
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
      body: "先核对已有参数 ID。",
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
  it("展示 Agent 保存的记忆并允许用户停用", async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/memory", component: MemoryPage }],
    });
    await router.push("/memory");
    await router.isReady();
    render({ template: "<RouterView />" }, { global: { plugins: [router] } });

    expect(await screen.findByText("眼睛参数")).toBeTruthy();
    expect(screen.getByText("参数命名经验")).toBeTruthy();
    expect(bridge.listMemories).toHaveBeenCalledWith(null);

    await fireEvent.click(screen.getAllByRole("switch", { name: "启用" })[0]);
    expect(bridge.setMemoryEnabled).toHaveBeenCalledWith("project-memory", false);
  });
});
