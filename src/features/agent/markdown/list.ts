import { parseInlineMarkdownLines } from "./inline";
import type { MarkdownListNode, ParsedListItem } from "./types";

interface DraftListItem {
  lines: string[];
  taskChecked: boolean | null;
  children: DraftListNode[];
}

interface DraftListNode {
  ordered: boolean;
  start: number | null;
  items: DraftListItem[];
}

export function parseListBlock(
  lines: string[],
  startIndex: number,
  first: ParsedListItem,
  isBlockStart: (line: string, lines?: string[], index?: number) => boolean,
): { node: MarkdownListNode; nextIndex: number } {
  const root = makeDraftList(first.ordered, first.number);
  const stack: Array<{ indent: number; node: DraftListNode }> = [
    { indent: first.indent, node: root },
  ];
  let lastItem: DraftListItem | null = null;
  let index = startIndex;

  while (index < lines.length) {
    const line = lines[index] ?? "";
    if (!line.trim()) break;
    const item = index === startIndex ? first : parseListItem(line);
    if (item) {
      if (item.indent < first.indent) break;
      while (stack.length > 1 && item.indent < stack[stack.length - 1]!.indent) stack.pop();
      let current = stack[stack.length - 1]!;
      if (item.indent > current.indent) {
        if (!lastItem) break;
        const child = makeDraftList(item.ordered, item.number);
        lastItem.children.push(child);
        current = { indent: item.indent, node: child };
        stack.push(current);
      } else if (item.ordered !== current.node.ordered && current.node.items.length) {
        break;
      }
      lastItem = { lines: [item.text], taskChecked: item.taskChecked, children: [] };
      current.node.items.push(lastItem);
      index += 1;
      continue;
    }

    const indent = line.match(/^ */)?.[0].length ?? 0;
    if (!lastItem || indent <= first.indent || isBlockStart(line, lines, index)) break;
    lastItem.lines.push(line.slice(Math.min(indent, first.indent + 2)).trim());
    index += 1;
  }

  return { node: finalize(root), nextIndex: index };
}

function makeDraftList(ordered: boolean, start: number | null): DraftListNode {
  return { ordered, start: ordered ? start : null, items: [] };
}

function finalize(node: DraftListNode): MarkdownListNode {
  return {
    ordered: node.ordered,
    start: node.start,
    items: node.items.map((item) => ({
      inlines: parseInlineMarkdownLines(item.lines),
      taskChecked: item.taskChecked,
      children: item.children.map(finalize),
    })),
  };
}

export function parseListItem(line: string): ParsedListItem | null {
  const match = line.match(/^( *)(?:(\d+)[.)]|([-*+]))\s+(.+)$/);
  if (!match) return null;
  const text = (match[4] ?? "").trim();
  const task = text.match(/^\[([ xX])\]\s+(.*)$/);
  return {
    indent: match[1]?.length ?? 0,
    ordered: match[2] !== undefined,
    number: match[2] === undefined ? null : Number.parseInt(match[2], 10),
    text: task ? (task[2] ?? "").trim() : text,
    taskChecked: task ? (task[1] ?? "").toLowerCase() === "x" : null,
  };
}
