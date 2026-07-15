import { describe, expect, it } from "vitest";
import { searchConversations } from "../src/features/agent/conversationSearch";
import type { ConversationSummary } from "../src/features/agent/types";

describe("会话标题搜索", () => {
  it("优先返回子串命中，再按轻量模糊匹配补充", () => {
    const rows = [
      summary("fuzzy", "Cubism 参数编辑"),
      summary("exact", "修复会话搜索"),
      summary("other", "Editor 连接"),
    ];

    const results = searchConversations(rows, "会话搜索");

    expect(results[0]).toMatchObject({ id: "exact", highlights: [[2, 6]] });
    expect(results.some((row) => row.id === "other")).toBe(false);
  });

  it("不在空查询时暴露全部会话", () => {
    expect(searchConversations([summary("a", "会话 A")], "   ")).toEqual([]);
  });
});

function summary(id: string, title: string): ConversationSummary {
  return {
    id,
    title,
    projectId: null,
    projectName: null,
    updatedAt: "2026-07-15T00:00:00Z",
    pinned: false,
  };
}
