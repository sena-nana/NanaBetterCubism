import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import type { ConversationSummary } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  deleteConversation: vi.fn(async () => undefined),
  listConversations: vi.fn<() => Promise<ConversationSummary[]>>(async () => []),
  listenConversationsChanged: vi.fn(async () => () => undefined),
  normalizeCommandError: vi.fn((error: unknown) => ({
    code: "test",
    message: error instanceof Error ? error.message : String(error),
  })),
  setConversationPinned: vi.fn(async () => true),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

beforeEach(() => {
  vi.resetModules();
  bridge.deleteConversation.mockClear();
  bridge.listConversations.mockReset();
  bridge.listConversations.mockResolvedValue([]);
});

describe("侧边栏顶部会话工具", () => {
  it("新对话只返回空白入口，搜索支持方向键、Enter 和 Esc", async () => {
    const { default: ConversationSidebarTop } = await import(
      "../src/features/agent/components/ConversationSidebarTop.vue"
    );
    const { applyConversationGroup } = await import(
      "../src/features/agent/sidebarConversations"
    );
    applyConversationGroup([
      summary("a", "修复参数", "项目 A"),
      summary("b", "参数搜索", null),
    ]);
    const router = createTestRouter();
    await router.push("/chats/a");
    await router.isReady();
    render(ConversationSidebarTop, { global: { plugins: [router] } });

    await fireEvent.click(screen.getByRole("button", { name: "新对话" }));
    await waitFor(() => expect(router.currentRoute.value.path).toBe("/"));

    await fireEvent.click(screen.getByRole("button", { name: "搜索会话" }));
    const input = screen.getByPlaceholderText("搜索会话…");
    await fireEvent.update(input, "参数");
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(router.currentRoute.value.path).toBe("/chats/a"));

    await fireEvent.click(screen.getByRole("button", { name: "搜索会话" }));
    await fireEvent.keyDown(screen.getByPlaceholderText("搜索会话…"), { key: "Escape" });
    expect(screen.queryByPlaceholderText("搜索会话…")).toBeNull();
  });

  it("彻底删除当前会话后回到空白新对话页", async () => {
    const { default: ConversationSidebarTop } = await import(
      "../src/features/agent/components/ConversationSidebarTop.vue"
    );
    const { applyConversationGroup, requestConversationDelete } = await import(
      "../src/features/agent/sidebarConversations"
    );
    const row = summary("a", "待删除", null);
    applyConversationGroup([row]);
    requestConversationDelete(row);
    const router = createTestRouter();
    await router.push("/chats/a");
    await router.isReady();
    render(ConversationSidebarTop, { global: { plugins: [router] } });

    await fireEvent.click(screen.getByRole("button", { name: "彻底删除" }));

    expect(bridge.deleteConversation).toHaveBeenCalledWith("a");
    await waitFor(() => expect(router.currentRoute.value.path).toBe("/"));
  });
});

function createTestRouter() {
  const Page = defineComponent({ template: "<div />" });
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: Page },
      { path: "/chats/:id", component: Page },
    ],
  });
}

function summary(
  id: string,
  title: string,
  projectName: string | null,
): ConversationSummary {
  return {
    id,
    title,
    projectId: projectName ? `project-${id}` : null,
    projectName,
    updatedAt: "2026-07-15T00:00:00Z",
    pinned: false,
  };
}
