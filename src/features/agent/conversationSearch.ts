import type { ConversationSummary } from "./types";

interface ConversationSearchResult extends ConversationSummary {
  highlights: Array<[number, number]>;
  score: number;
}

export function searchConversations(
  rows: readonly ConversationSummary[],
  query: string,
): ConversationSearchResult[] {
  const normalizedQuery = normalize(query);
  if (!normalizedQuery) return [];
  const queryBigrams = bigrams(normalizedQuery);

  return rows
    .map((row) => rankConversation(row, normalizedQuery, queryBigrams))
    .filter((result): result is ConversationSearchResult => result !== null)
    .sort((left, right) => right.score - left.score || right.updatedAt.localeCompare(left.updatedAt));
}

function rankConversation(
  row: ConversationSummary,
  query: string,
  queryBigrams: Map<string, number>,
): ConversationSearchResult | null {
  const title = normalize(row.title);
  const highlights = findHighlights(row.title, query);
  if (highlights.length > 0) {
    const earliest = Math.min(...highlights.map(([start]) => start));
    return {
      ...row,
      highlights,
      score: 2 + highlights.length + 1 - earliest / Math.max(title.length, 1),
    };
  }

  const similarity = cosine(queryBigrams, bigrams(title));
  if (similarity <= 0) return null;
  return { ...row, highlights: [], score: similarity };
}

function findHighlights(title: string, query: string): Array<[number, number]> {
  const normalizedTitle = title.toLowerCase();
  const exact: Array<[number, number]> = [];
  let offset = 0;
  while ((offset = normalizedTitle.indexOf(query, offset)) >= 0) {
    exact.push([offset, offset + query.length]);
    offset += query.length;
  }
  if (exact.length > 0) return exact;

  const tokens = query.split(/\s+/).filter(Boolean);
  const tokenRanges: Array<[number, number]> = [];
  for (const token of tokens) {
    const index = normalizedTitle.indexOf(token);
    if (index >= 0) tokenRanges.push([index, index + token.length]);
  }
  return tokenRanges.length === tokens.length ? tokenRanges : [];
}

function normalize(value: string) {
  return value.toLowerCase().replace(/\s+/g, " ").trim();
}

function bigrams(value: string) {
  const terms = new Map<string, number>();
  if (!value) return terms;
  if (value.length === 1) {
    terms.set(value, 1);
    return terms;
  }
  for (let index = 0; index < value.length - 1; index += 1) {
    const term = value.slice(index, index + 2);
    terms.set(term, (terms.get(term) ?? 0) + 1);
  }
  return terms;
}

function cosine(left: Map<string, number>, right: Map<string, number>) {
  let dot = 0;
  let leftNorm = 0;
  let rightNorm = 0;
  for (const value of left.values()) leftNorm += value * value;
  for (const value of right.values()) rightNorm += value * value;
  for (const [term, value] of left) dot += value * (right.get(term) ?? 0);
  if (dot === 0 || leftNorm === 0 || rightNorm === 0) return 0;
  return dot / Math.sqrt(leftNorm * rightNorm);
}
