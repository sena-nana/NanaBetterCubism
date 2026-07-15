import type { TableAlignment } from "./types";

interface ParsedTable {
  headers: string[];
  alignments: TableAlignment[];
  rows: string[][];
  nextIndex: number;
}

export function isTableStart(lines: string[], startIndex: number) {
  return parseTable(lines, startIndex) !== null;
}

export function parseTable(lines: string[], startIndex: number): ParsedTable | null {
  const header = parseTableRow(lines[startIndex] ?? "");
  if (!header) return null;
  const alignments = parseDelimiter(lines[startIndex + 1] ?? "", header.length);
  if (!alignments) return null;
  const rows: string[][] = [];
  let index = startIndex + 2;
  while (index < lines.length) {
    const row = parseTableRow(lines[index] ?? "");
    if (!row) break;
    rows.push(normalizeCells(row, alignments.length));
    index += 1;
  }
  return {
    headers: normalizeCells(header, alignments.length),
    alignments,
    rows,
    nextIndex: index,
  };
}

function parseDelimiter(line: string, expected: number): TableAlignment[] | null {
  const cells = parseTableRow(line);
  if (!cells || cells.length !== expected) return null;
  const result: TableAlignment[] = [];
  for (const cell of cells) {
    const value = cell.trim();
    if (!/^:?-{3,}:?$/.test(value)) return null;
    const left = value.startsWith(":");
    const right = value.endsWith(":");
    result.push(left && right ? "center" : right ? "right" : left ? "left" : null);
  }
  return result;
}

function parseTableRow(line: string): string[] | null {
  let body = line.trim();
  if (!body.includes("|")) return null;
  if (body.startsWith("|")) body = body.slice(1);
  if (body.endsWith("|")) body = body.slice(0, -1);
  const cells: string[] = [];
  let current = "";
  for (let index = 0; index < body.length; index += 1) {
    const char = body[index] ?? "";
    if (char === "\\" && body[index + 1] === "|") {
      current += "|";
      index += 1;
    } else if (char === "|") {
      cells.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current.trim());
  return cells;
}

function normalizeCells(cells: string[], count: number) {
  const normalized = cells.slice(0, count);
  while (normalized.length < count) normalized.push("");
  return normalized;
}
