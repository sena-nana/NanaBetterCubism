import { describe, expect, it } from "vitest";
import { extractMemoryOverview } from "../src/features/agent/memoryMarkdown";

describe("memoryMarkdown", () => {
  it("抽取 Overview / Summary，无分层时回退纯文本", () => {
    expect(
      extractMemoryOverview(
        "project",
        "# t\n\n## Overview\n已完成。\n\n## Stage\n细节\n",
      ),
    ).toBe("已完成。");
    expect(extractMemoryOverview("global", "## Summary\n先核对 ID。\n")).toBe("先核对 ID。");
    expect(extractMemoryOverview("global", "纯文本经验")).toBe("纯文本经验");
  });
});
