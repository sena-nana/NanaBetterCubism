import { fireEvent, render, screen } from "@testing-library/vue";
import { describe, expect, it } from "vitest";
import ConversationTranscript from "../src/features/agent/components/ConversationTranscript.vue";
import {
  buildConversationTimeline,
  toolActivityGroupPresentation,
} from "../src/features/agent/toolActivityGroups";
import type { ChatMessage } from "../src/features/agent/types";

type ToolMessageFixture = ChatMessage & { toolDisplayName?: string | null };

function message(overrides: Partial<ToolMessageFixture>): ChatMessage {
  return {
    id: "message-1",
    role: "assistant",
    content: "content",
    toolName: null,
    toolDisplayName: null,
    toolStatus: null,
    createdAt: "2026-07-16T00:00:00Z",
    ...overrides,
  } as ToolMessageFixture;
}

describe("工具调用折叠展示", () => {
  it("只聚合相邻工具消息，并以普通消息作为分组边界", () => {
    const first = message({
      id: "tool-a",
      role: "tool",
      toolName: "get_editor_snapshot",
      toolDisplayName: "检查 Editor 状态",
      toolStatus: "finished",
    });
    const second = message({
      id: "tool-b",
      role: "tool",
      toolName: "preview_parameter_batch",
      toolDisplayName: "预览参数修改",
      toolStatus: "finished",
    });
    const third = message({
      id: "tool-c",
      role: "tool",
      toolName: "execute_parameter_batch",
      toolDisplayName: "应用参数修改",
      toolStatus: "finished",
    });
    const timeline = buildConversationTimeline([
      message({ id: "assistant-a", content: "开始检查" }),
      first,
      second,
      message({ id: "assistant-b", content: "检查完成" }),
      third,
    ]);

    expect(timeline.map((entry) => entry.kind)).toEqual([
      "message",
      "tool-group",
      "message",
      "tool-group",
    ]);
    expect(timeline[1]?.kind === "tool-group" && timeline[1].messages).toEqual([first, second]);
    expect(timeline[3]?.kind === "tool-group" && timeline[3].messages).toEqual([third]);
    expect(toolActivityGroupPresentation([first, second]).mode).toBe("multiple");
    expect(toolActivityGroupPresentation([third]).mode).toBe("single");
  });

  it("运行期间复用单行并跟随当前操作，完成后保持同一展开组", async () => {
    const firstStarted = message({
      id: "tool-a",
      role: "tool",
      toolName: "find_selected_part_parameters",
      toolDisplayName: "读取选中 Part 参数",
      toolStatus: "started",
    });
    const view = render(ConversationTranscript, { props: { messages: [firstStarted] } });

    const initialGroup = view.container.querySelector<HTMLElement>("[data-summary-mode='current']")!;
    expect(initialGroup.dataset.operationCount).toBe("1");
    expect(initialGroup.textContent).toContain("读取选中 Part 参数");
    expect(view.container.querySelector("[data-agent-id='agent.chat.tool.tool-a']")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByRole("button", { expanded: true })).toBeTruthy();
    expect(view.container.querySelector("[data-agent-id='agent.chat.tool.tool-a']")).toBeTruthy();

    await view.rerender({
      messages: [
        { ...firstStarted, toolStatus: "finished" },
        message({
          id: "tool-b",
          role: "tool",
          toolName: "execute_parameter_batch",
          toolDisplayName: "应用参数修改",
          toolStatus: "started",
        }),
      ],
    });
    expect(screen.getByRole("button", { expanded: true }).textContent).toContain("应用参数修改");
    expect(view.container.querySelectorAll("[data-summary-mode]")).toHaveLength(1);

    await view.rerender({
      messages: [
        { ...firstStarted, toolStatus: "finished" },
        message({
          id: "tool-b",
          role: "tool",
          toolName: "execute_parameter_batch",
          toolDisplayName: "应用参数修改",
          toolStatus: "finished",
        }),
      ],
    });
    const completedGroup = view.container.querySelector<HTMLElement>("[data-summary-mode='multiple']")!;
    expect(completedGroup).toBe(initialGroup);
    expect(completedGroup.dataset.status).toBe("finished");
    expect(completedGroup.dataset.operationCount).toBe("2");
    expect(screen.getByRole("button", { expanded: true })).toBeTruthy();
  });

  it("失败组默认折叠，展开后只暴露失败详情", async () => {
    const successfulPayload = "successful-private-payload";
    const failure = "操作未提交";
    const view = render(ConversationTranscript, {
      props: {
        messages: [
          message({
            id: "tool-a",
            role: "tool",
            content: successfulPayload,
            toolName: "get_editor_snapshot",
            toolDisplayName: "检查 Editor 状态",
            toolStatus: "finished",
          }),
          message({
            id: "tool-b",
            role: "tool",
            content: failure,
            toolName: "execute_parameter_batch",
            toolDisplayName: "应用参数修改",
            toolStatus: "failed",
          }),
        ],
      },
    });

    const group = view.container.querySelector<HTMLElement>("[data-summary-mode='failed']")!;
    expect(group.dataset.status).toBe("failed");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByText(successfulPayload)).toBeNull();

    await fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByRole("alert").textContent).toBe(failure);
    expect(screen.queryByText(successfulPayload)).toBeNull();
    expect(view.container.querySelectorAll("[data-agent-id^='agent.chat.tool.tool-']")).toHaveLength(2);
  });
});
