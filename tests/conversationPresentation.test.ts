import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it, vi } from "vitest";
import MarkdownBlock from "../src/features/agent/markdown/MarkdownBlock.vue";
import PlanTodoPanel from "../src/features/agent/components/PlanTodoPanel.vue";
import ConversationTranscript from "../src/features/agent/components/ConversationTranscript.vue";
import { toolActivityPresentation } from "../src/features/agent/conversationPresentation";
import type { ChatMessage, ConversationPlan } from "../src/features/agent/types";

const opener = vi.hoisted(() => ({ openUrl: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/plugin-opener", () => opener);

function message(overrides: Partial<ChatMessage>): ChatMessage {
  return {
    id: "message-1",
    role: "assistant",
    content: "content",
    toolName: null,
    toolDisplayName: null,
    toolStatus: null,
    createdAt: "2026-07-15T00:00:00Z",
    ...overrides,
  };
}

function plan(steps: ConversationPlan["steps"]): ConversationPlan {
  return {
    conversationId: "conversation-1",
    steps,
    updatedAt: "2026-07-15T00:00:00Z",
  };
}

describe("成熟对话展示", () => {
  it("以安全节点渲染基础 Markdown，并通过受限 opener 打开链接", async () => {
    const view = render(MarkdownBlock, {
      props: {
        content: [
          "# 标题",
          "- [x] 已完成",
          "- [ ] 待处理",
          "",
          "| 项目 | 状态 |",
          "| --- | --- |",
          "| 参数 | 正常 |",
          "",
          "```json",
          "{\"ok\":true}",
          "```",
          "",
          "[文档](https://example.com/docs)",
          "[危险](javascript:alert(1))",
          "<script>window.__unsafe = true</script>",
        ].join("\n"),
      },
    });

    expect(screen.getByRole("heading", { level: 2 })).toBeTruthy();
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    expect(view.container.querySelector("table")).toBeTruthy();
    expect(view.container.querySelector("pre code")?.textContent).toContain("ok");
    expect(view.container.querySelector("script")).toBeNull();
    expect(screen.queryByRole("button", { name: "危险" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "文档" }));
    expect(opener.openUrl).toHaveBeenCalledWith("https://example.com/docs");
  });

  it("计划面板默认只显示待办，展开后显示完整计划", async () => {
    const view = render(PlanTodoPanel, {
      props: {
        plan: plan([
          { id: "active", title: "正在处理的步骤", status: "in_progress" },
          { id: "pending", title: "后续步骤", status: "pending" },
          { id: "done", title: "完成步骤", status: "completed" },
        ]),
      },
    });

    expect(screen.getByText("正在处理的步骤")).toBeTruthy();
    expect(screen.getByText("后续步骤")).toBeTruthy();
    expect(screen.queryByText("完成步骤")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByText("完成步骤")).toBeTruthy();

    await view.rerender({
      plan: plan([{ id: "done", title: "完成步骤", status: "completed" }]),
    });
    await fireEvent.click(screen.getByRole("button", { expanded: true }));
    expect(screen.getByRole("status")).toBeTruthy();
  });

  it("用户向上阅读时不抢滚动，并能主动回到最新", async () => {
    const view = render(ConversationTranscript, {
      props: { messages: [message({ id: "a", content: "第一条" })] },
    });
    await Promise.resolve();
    await Promise.resolve();
    const scroller = view.container.querySelector<HTMLElement>("[data-agent-id='agent.chat.transcript']")!;
    Object.defineProperty(scroller, "scrollHeight", { configurable: true, value: 1000 });
    Object.defineProperty(scroller, "clientHeight", { configurable: true, value: 200 });
    scroller.scrollTop = 100;
    await fireEvent.scroll(scroller);

    expect(screen.getByRole("button", { name: "回到最新" })).toBeTruthy();
    await view.rerender({
      messages: [
        message({ id: "a", content: "第一条" }),
        message({ id: "b", content: "第二条" }),
      ],
    });
    await Promise.resolve();
    expect(scroller.scrollTop).toBe(100);

    await fireEvent.click(screen.getByRole("button", { name: "回到最新" }));
    expect(scroller.scrollTop).toBe(1000);
  });

  it("工具成功状态隐藏原始结果，失败状态保留真实错误", () => {
    const finished = toolActivityPresentation(message({
      role: "tool",
      toolName: "get_editor_snapshot",
      toolDisplayName: "检查 Editor 状态",
      toolStatus: "finished",
      content: "{\"state\":\"ready\"}",
    }));
    const failed = toolActivityPresentation(message({
      role: "tool",
      toolName: "execute_parameter_batch",
      toolDisplayName: "应用参数修改",
      toolStatus: "failed",
      content: "操作未提交",
    }));

    expect(finished.status).toBe("finished");
    expect(finished.label).toBe("检查 Editor 状态");
    expect(finished.detail).toBeNull();
    expect(failed.status).toBe("failed");
    expect(failed.detail).toBe("操作未提交");
  });

  it("未识别的历史工具保持真实未知状态", () => {
    const unknown = toolActivityPresentation(message({
      role: "tool",
      toolName: "retired_tool",
      toolDisplayName: null,
      toolStatus: "finished",
    }));

    expect(unknown.label).toBe("未知工具");
  });

});
