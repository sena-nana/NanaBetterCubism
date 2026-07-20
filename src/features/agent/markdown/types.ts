export type InlineTokenType =
  | "text"
  | "code"
  | "strong"
  | "em"
  | "link"
  | "break";

export type TableAlignment = "left" | "center" | "right" | null;

export interface InlineToken {
  type: InlineTokenType;
  text: string;
  href: string | null;
}

export interface MarkdownListItem {
  inlines: InlineToken[];
  taskChecked: boolean | null;
  children: MarkdownListNode[];
}

export interface MarkdownListNode {
  ordered: boolean;
  start: number | null;
  items: MarkdownListItem[];
}

export interface MarkdownBlockNode {
  key: string;
  type: "paragraph" | "heading" | "code" | "mermaid" | "list" | "quote" | "table";
  inlines: InlineToken[];
  text: string;
  language: string;
  list: MarkdownListNode | null;
  level: 2 | 3 | 4 | 5 | 6;
  alignments: TableAlignment[];
  headers: InlineToken[][];
  rows: InlineToken[][][];
}

export interface ParsedListItem {
  indent: number;
  ordered: boolean;
  number: number | null;
  text: string;
  taskChecked: boolean | null;
}

export function makeInlineToken(
  type: InlineTokenType,
  text: string,
  href: string | null = null,
): InlineToken {
  return { type, text, href };
}

export function makeBlock(
  type: MarkdownBlockNode["type"],
  key: string,
  overrides: Partial<Omit<MarkdownBlockNode, "type" | "key">> = {},
): MarkdownBlockNode {
  return {
    key,
    type,
    inlines: [],
    text: "",
    language: "",
    list: null,
    level: 2,
    alignments: [],
    headers: [],
    rows: [],
    ...overrides,
  };
}
