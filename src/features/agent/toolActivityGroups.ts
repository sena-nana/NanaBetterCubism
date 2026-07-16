import {
  toolActivityPresentation,
  type ToolActivityPresentation,
} from "./conversationPresentation";
import type { ChatMessage } from "./types";

export interface ConversationMessageEntry {
  kind: "message";
  key: string;
  message: ChatMessage;
}

export interface ConversationToolGroupEntry {
  kind: "tool-group";
  key: string;
  messages: ChatMessage[];
}

export type ConversationTimelineEntry = ConversationMessageEntry | ConversationToolGroupEntry;

export interface ToolActivityGroupPresentation extends ToolActivityPresentation {
  count: number;
  mode: "current" | "single" | "multiple" | "failed";
}

export function buildConversationTimeline(messages: ChatMessage[]): ConversationTimelineEntry[] {
  const timeline: ConversationTimelineEntry[] = [];
  for (const message of messages) {
    const previous = timeline[timeline.length - 1];
    if (message.role === "tool") {
      if (previous?.kind === "tool-group") {
        previous.messages.push(message);
      } else {
        timeline.push({
          kind: "tool-group",
          key: `tool-group:${message.id}`,
          messages: [message],
        });
      }
      continue;
    }
    timeline.push({ kind: "message", key: message.id, message });
  }
  return timeline;
}

export function toolActivityGroupPresentation(
  messages: ChatMessage[],
): ToolActivityGroupPresentation {
  const items = messages.map(toolActivityPresentation);
  const current = [...items].reverse().find((item) => item.status === "started");
  if (current) {
    return { ...current, count: items.length, mode: "current", detail: null };
  }

  const failed = items.filter((item) => item.status === "failed");
  if (failed.length) {
    const onlyItem = items.length === 1;
    return {
      label: onlyItem
        ? failed[0].label
        : failed.length === items.length
          ? "操作失败"
          : "部分操作失败",
      detail: null,
      status: "failed",
      count: items.length,
      mode: "failed",
    };
  }

  if (items.length > 1 && items.every((item) => item.status === "finished")) {
    return {
      label: "进行了多个操作",
      detail: null,
      status: "finished",
      count: items.length,
      mode: "multiple",
    };
  }

  const item = items[items.length - 1] ?? {
    label: "执行操作",
    detail: null,
    status: "unknown" as const,
  };
  return { ...item, count: items.length, mode: "single" };
}
