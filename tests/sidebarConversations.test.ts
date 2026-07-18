import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationSummary } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  deleteConversation: vi.fn<(conversationId: string) => Promise<void>>(),
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
  bridge.deleteConversation.mockReset();
  bridge.setConversationPinned.mockReset();
});

describe("Agent 侧边栏会话", () => {
  it("按项目与收集箱分组，并保留各组的后端顺序", async () => {
    const { applyConversationGroup, sidebarConversationSections } = await import("../src/features/agent/sidebarConversations");
    const rows = [
      summary("a", "项目 A - 最近", "项目 A"),
      summary("b", "收集箱 - 最近", null),
      summary("c", "项目 A - 较早", "项目 A"),
      summary("d", "项目 B", "项目 B"),
      summary("e", "收集箱 - 较早", null),
    ];

    applyConversationGroup(rows);

    expect(sidebarConversationSections.map((group) => group.title)).toEqual(["项目 A", "收集箱", "项目 B"]);
    expect(sidebarConversationSections[0]?.items?.map((item) => item.key)).toEqual(["a", "c"]);
    expect(sidebarConversationSections[1]?.items?.map((item) => item.key)).toEqual(["b", "e"]);
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
    const { sidebarConversationSections } = await import("../src/features/agent/sidebarConversations");
    const { publishModelFooter, publishEditorFooter } = await import(
      "../src/features/shell/footerSelfCheck"
    );
    publishModelFooter({
      label: "example-model",
      title: "已保存模型 example-model。点击进入设置。",
      tone: "ok",
    });
    publishEditorFooter({
      label: "Editor 未连接",
      title: "在设置中连接 Cubism Editor。",
      tone: "warn",
    });
    const { editorFooterStatus } = await import("../src/features/shell/footerSelfCheck");
    expect(editorFooterStatus).toMatchObject({
      label: "Editor 未连接",
      tone: "warn",
      to: "/settings?tab=editor",
    });

    const loading = ensureSidebarConversationsLoaded();
    expect(sidebarConversationSections[0]?.emptyText).toBe("正在加载对话…");
    resolveRows([]);
    await loading;

    expect(sidebarConversationSections).toHaveLength(1);
    expect(sidebarConversationSections[0]).toMatchObject({ title: "收集箱", emptyText: "暂无对话" });
    expect(editorFooterStatus).toMatchObject({ label: "Editor 未连接", tone: "warn" });
  });

  it("壳层重新安装后用成功缓存恢复分组，不重复请求", async () => {
    bridge.listConversations.mockResolvedValue([summary("a", "缓存会话", null)]);
    const { installAgentShell, ensureSidebarConversationsLoaded, sidebarConversationSections } = await import(
      "../src/features/agent/sidebarConversations"
    );
    installAgentShell();
    await ensureSidebarConversationsLoaded();
    sidebarConversationSections.splice(0);
    installAgentShell();

    expect(sidebarConversationSections[0]?.items?.map((item) => item.key)).toEqual(["a"]);
    expect(bridge.listConversations).toHaveBeenCalledOnce();
  });

  it("加载失败后提供真实重试，且不会缓存 rejected promise", async () => {
    bridge.listConversations
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockResolvedValueOnce([summary("a", "已恢复", null)]);
    const { ensureSidebarConversationsLoaded, sidebarConversationSections } = await import(
      "../src/features/agent/sidebarConversations"
    );
    await expect(ensureSidebarConversationsLoaded()).rejects.toThrow("unavailable");
    expect(sidebarConversationSections[0]?.emptyText).toBe("无法加载对话");
    expect(sidebarConversationSections[0]?.tools?.[0]?.label).toBe("重试");

    await sidebarConversationSections[0]?.tools?.[0]?.onSelect?.();

    expect(bridge.listConversations).toHaveBeenCalledTimes(2);
    expect(sidebarConversationSections[0]?.items?.map((item) => item.key)).toEqual(["a"]);
  });

  it("置顶后立即更新排序和 active 状态", async () => {
    const row = summary("a", "已置顶", null);
    bridge.setConversationPinned.mockResolvedValue(true);
    const { applyConversationGroup, sidebarConversationSections, toggleConversationPinned } = await import(
      "../src/features/agent/sidebarConversations"
    );
    applyConversationGroup([summary("b", "其他会话", null), row]);
    await toggleConversationPinned(row);

    expect(bridge.setConversationPinned).toHaveBeenCalledWith("a", true);
    expect(sidebarConversationSections[0]?.items?.map((item) => item.key)).toEqual(["a", "b"]);
    expect(sidebarConversationSections[0]?.items?.[0]?.tools?.[0]).toMatchObject({
      label: "取消置顶",
      active: true,
    });
  });

  it("删除会话经确认后清理运行状态并从缓存列表移除", async () => {
    const row = summary("a", "待删除", null);
    bridge.deleteConversation.mockResolvedValue(undefined);
    const {
      applyConversationGroup,
      confirmConversationDelete,
      requestConversationDelete,
      sidebarConversationsState,
    } = await import("../src/features/agent/sidebarConversations");
    const { getConversationTurnPhase, setConversationTurnPhase } = await import(
      "../src/features/agent/conversationRuntimeStore"
    );

    applyConversationGroup([row]);
    setConversationTurnPhase("a", "running");
    requestConversationDelete(row);
    expect(sidebarConversationsState.deleteTarget?.id).toBe("a");
    await expect(confirmConversationDelete()).resolves.toBe("a");

    expect(bridge.deleteConversation).toHaveBeenCalledWith("a");
    expect(sidebarConversationsState.rows).toEqual([]);
    expect(getConversationTurnPhase("a")).toBe("idle");
  });

  it("删除失败时保留会话和确认目标", async () => {
    const row = summary("a", "待删除", null);
    bridge.deleteConversation.mockRejectedValue(new Error("unavailable"));
    const {
      applyConversationGroup,
      confirmConversationDelete,
      requestConversationDelete,
      sidebarConversationsState,
    } = await import("../src/features/agent/sidebarConversations");

    applyConversationGroup([row]);
    requestConversationDelete(row);
    await expect(confirmConversationDelete()).resolves.toBeNull();

    expect(sidebarConversationsState.rows).toEqual([row]);
    expect(sidebarConversationsState.deleteTarget).toEqual(row);
    expect(sidebarConversationsState.actionError).toBe("unavailable");
  });

  it("运行中的会话禁用删除操作", async () => {
    const row = summary("a", "运行中", null);
    const { applyConversationGroup, sidebarConversationSections } = await import("../src/features/agent/sidebarConversations");
    const { setConversationTurnPhase } = await import(
      "../src/features/agent/conversationRuntimeStore"
    );
    setConversationTurnPhase("a", "running");
    applyConversationGroup([row]);

    expect(sidebarConversationSections[0]?.items?.[0]?.tools?.[1]).toMatchObject({
      key: "delete",
      disabled: true,
    });
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
