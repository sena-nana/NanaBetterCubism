import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationSummary } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  archiveConversation: vi.fn<(conversationId: string) => Promise<boolean>>(),
  listConversations: vi.fn<() => Promise<ConversationSummary[]>>(),
  listenConversationsChanged: vi.fn(async () => () => undefined),
  normalizeCommandError: vi.fn((error: unknown) => ({
    code: "test",
    message: error instanceof Error ? error.message : String(error),
  })),
  setConversationPinned: vi.fn<(conversationId: string, pinned: boolean) => Promise<boolean>>(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

beforeEach(() => {
  vi.resetModules();
  bridge.listConversations.mockReset();
  bridge.listenConversationsChanged.mockClear();
  bridge.archiveConversation.mockReset();
  bridge.setConversationPinned.mockReset();
});

describe("Agent 侧边栏会话", () => {
  it("按项目与收集箱分组，并保留各组的后端顺序", async () => {
    const { applyConversationGroup } = await import("../src/features/agent/sidebarConversations");
    const { SIDEBAR_GROUPS } = await import("@lilia/ui");
    const rows = [
      summary("a", "项目 A - 最近", "项目 A"),
      summary("b", "收集箱 - 最近", null),
      summary("c", "项目 A - 较早", "项目 A"),
      summary("d", "项目 B", "项目 B"),
      summary("e", "收集箱 - 较早", null),
    ];

    applyConversationGroup(rows);

    expect(SIDEBAR_GROUPS.map((group) => group.title)).toEqual(["项目 A", "收集箱", "项目 B"]);
    expect(SIDEBAR_GROUPS[0]?.items?.map((item) => item.key)).toEqual(["a", "c"]);
    expect(SIDEBAR_GROUPS[1]?.items?.map((item) => item.key)).toEqual(["b", "e"]);
  });

  it("展示加载与空状态，且刷新会话不覆盖 Editor footer", async () => {
    let resolveRows!: (rows: ConversationSummary[]) => void;
    bridge.listConversations.mockReturnValue(
      new Promise((resolve) => {
        resolveRows = resolve;
      }),
    );
    const { ensureSidebarConversationsLoaded } = await import(
      "../src/features/agent/sidebarConversations"
    );
    const { SIDEBAR_FOOTER_STATUS, SIDEBAR_GROUPS } = await import("@lilia/ui");
    Object.assign(SIDEBAR_FOOTER_STATUS, {
      to: "/settings?tab=editor",
      label: "Editor 已就绪",
      title: "已连接",
      tone: "ok",
    });

    const loading = ensureSidebarConversationsLoaded();
    expect(SIDEBAR_GROUPS[0]?.emptyText).toBe("正在加载对话…");
    resolveRows([]);
    await loading;

    expect(SIDEBAR_GROUPS).toHaveLength(1);
    expect(SIDEBAR_GROUPS[0]).toMatchObject({ title: "收集箱", emptyText: "暂无对话" });
    expect(SIDEBAR_FOOTER_STATUS).toMatchObject({ label: "Editor 已就绪", tone: "ok" });
  });

  it("壳层重新安装后用成功缓存恢复分组，不重复请求", async () => {
    bridge.listConversations.mockResolvedValue([summary("a", "缓存会话", null)]);
    const { installAgentShell, ensureSidebarConversationsLoaded } = await import(
      "../src/features/agent/sidebarConversations"
    );
    const { SIDEBAR_GROUPS } = await import("@lilia/ui");

    installAgentShell();
    await ensureSidebarConversationsLoaded();
    SIDEBAR_GROUPS.splice(0);
    installAgentShell();

    expect(SIDEBAR_GROUPS[0]?.items?.map((item) => item.key)).toEqual(["a"]);
    expect(bridge.listConversations).toHaveBeenCalledOnce();
  });

  it("加载失败后提供真实重试，且不会缓存 rejected promise", async () => {
    bridge.listConversations
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockResolvedValueOnce([summary("a", "已恢复", null)]);
    const { ensureSidebarConversationsLoaded } = await import(
      "../src/features/agent/sidebarConversations"
    );
    const { SIDEBAR_GROUPS } = await import("@lilia/ui");

    await expect(ensureSidebarConversationsLoaded()).rejects.toThrow("unavailable");
    expect(SIDEBAR_GROUPS[0]?.emptyText).toBe("无法加载对话");
    expect(SIDEBAR_GROUPS[0]?.tools?.[0]?.label).toBe("重试");

    await SIDEBAR_GROUPS[0]?.tools?.[0]?.onSelect?.();

    expect(bridge.listConversations).toHaveBeenCalledTimes(2);
    expect(SIDEBAR_GROUPS[0]?.items?.map((item) => item.key)).toEqual(["a"]);
  });

  it("置顶后立即更新排序和 active 状态", async () => {
    const row = summary("a", "已置顶", null);
    bridge.setConversationPinned.mockResolvedValue(true);
    const { applyConversationGroup, toggleConversationPinned } = await import(
      "../src/features/agent/sidebarConversations"
    );
    const { SIDEBAR_GROUPS } = await import("@lilia/ui");

    applyConversationGroup([summary("b", "其他会话", null), row]);
    await toggleConversationPinned(row);

    expect(bridge.setConversationPinned).toHaveBeenCalledWith("a", true);
    expect(SIDEBAR_GROUPS[0]?.items?.map((item) => item.key)).toEqual(["a", "b"]);
    expect(SIDEBAR_GROUPS[0]?.items?.[0]?.tools?.[0]).toMatchObject({
      label: "取消置顶",
      active: true,
    });
  });

  it("归档会话经确认后从缓存列表移除", async () => {
    const row = summary("a", "待归档", null);
    bridge.archiveConversation.mockResolvedValue(true);
    const {
      applyConversationGroup,
      confirmConversationArchive,
      requestConversationArchive,
      sidebarConversationsState,
    } = await import("../src/features/agent/sidebarConversations");

    applyConversationGroup([row]);
    requestConversationArchive(row);
    expect(sidebarConversationsState.archiveTarget?.id).toBe("a");
    await expect(confirmConversationArchive()).resolves.toBe("a");

    expect(bridge.archiveConversation).toHaveBeenCalledWith("a");
    expect(sidebarConversationsState.rows).toEqual([]);
  });
});

function summary(id: string, title: string, projectName: string | null): ConversationSummary {
  return {
    id,
    title,
    projectId: projectName ? `project-${projectName}` : null,
    projectName,
    updatedAt: "2026-07-15T00:00:00Z",
    pinned: false,
  };
}
