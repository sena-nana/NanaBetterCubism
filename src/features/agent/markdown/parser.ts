import { parseInlineMarkdown, parseInlineMarkdownLines } from "./inline";
import { parseListBlock, parseListItem } from "./list";
import { isTableStart, parseTable } from "./table";
import { makeBlock, type MarkdownBlockNode } from "./types";

export type { InlineToken, MarkdownBlockNode, MarkdownListNode, TableAlignment } from "./types";

export function normalizeMarkdownSource(content: string | null | undefined) {
  return (content ?? "").replace(/\r\n?/g, "\n").trim();
}

export function parseMarkdownBlocks(source: string): MarkdownBlockNode[] {
  if (!source) return [];
  const lines = source.split("\n");
  const blocks: MarkdownBlockNode[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index] ?? "";
    const trimmed = line.trim();
    const key = `block-${blocks.length}`;
    if (!trimmed) {
      index += 1;
      continue;
    }

    const fence = parseFence(lines, index);
    if (fence) {
      const type = fence.closed && fence.language.toLowerCase() === "mermaid"
        ? "mermaid"
        : "code";
      blocks.push(makeBlock(type, key, { text: fence.text, language: fence.language }));
      index = fence.nextIndex;
      continue;
    }
    const heading = line.match(/^\s*(#{1,6})\s+(.+)$/);
    if (heading) {
      const level = Math.min(6, Math.max(2, (heading[1]?.length ?? 1) + 1)) as 2 | 3 | 4 | 5 | 6;
      blocks.push(makeBlock("heading", key, {
        level,
        inlines: parseInlineMarkdown((heading[2] ?? "").trim()),
      }));
      index += 1;
      continue;
    }
    const listItem = parseListItem(line);
    if (listItem) {
      const list = parseListBlock(lines, index, listItem, isBlockStart);
      blocks.push(makeBlock("list", key, { list: list.node }));
      index = list.nextIndex;
      continue;
    }
    if (/^\s*>\s?/.test(line)) {
      const quoteLines: string[] = [];
      while (index < lines.length && /^\s*>\s?/.test(lines[index] ?? "")) {
        quoteLines.push((lines[index] ?? "").replace(/^\s*>\s?/, "").trim());
        index += 1;
      }
      blocks.push(makeBlock("quote", key, {
        inlines: parseInlineMarkdownLines(quoteLines),
      }));
      continue;
    }
    const table = parseTable(lines, index);
    if (table) {
      blocks.push(makeBlock("table", key, {
        alignments: table.alignments,
        headers: table.headers.map(parseInlineMarkdown),
        rows: table.rows.map((row) => row.map(parseInlineMarkdown)),
      }));
      index = table.nextIndex;
      continue;
    }

    const paragraphLines: string[] = [];
    while (index < lines.length) {
      const paragraph = lines[index] ?? "";
      if (!paragraph.trim() || isBlockStart(paragraph, lines, index)) break;
      paragraphLines.push(paragraph);
      index += 1;
    }
    if (paragraphLines.length) {
      blocks.push(makeBlock("paragraph", key, {
        inlines: parseInlineMarkdownLines(paragraphLines),
      }));
    }
  }
  return blocks;
}

function parseFence(lines: string[], startIndex: number) {
  const match = (lines[startIndex] ?? "").match(/^\s*(```+|~~~+)\s*([A-Za-z0-9_-]*)?.*$/);
  if (!match) return null;
  const marker = match[1] ?? "```";
  const closingPattern = new RegExp(`^${marker[0]}{${marker.length},}\\s*$`);
  const code: string[] = [];
  let index = startIndex + 1;
  let closed = false;
  while (index < lines.length) {
    const line = lines[index] ?? "";
    if (closingPattern.test(line.trim())) {
      closed = true;
      index += 1;
      break;
    }
    code.push(line);
    index += 1;
  }
  return {
    text: code.join("\n").replace(/\n+$/, ""),
    language: match[2] ?? "",
    closed,
    nextIndex: index,
  };
}

function isBlockStart(line: string, lines?: string[], index?: number) {
  return /^\s*(```+|~~~+)/.test(line)
    || (lines !== undefined && index !== undefined && isTableStart(lines, index))
    || /^\s*(#{1,6})\s+/.test(line)
    || parseListItem(line) !== null
    || /^\s*>\s?/.test(line);
}
