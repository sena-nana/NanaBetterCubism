import { makeInlineToken, type InlineToken } from "./types";

const INLINE_PATTERN = /`(?<code>[^`\n]+)`|\*\*(?<starStrong>[^*\n]+)\*\*|__(?<underscoreStrong>[^_\n]+)__|_(?<underscoreEm>[^_\n]+)_|\*(?<starEm>[^*\n]+)\*|\[(?<linkText>[^\]\n]+)\]\((?<linkHref>[^)\s]+)\)|<(?<angleHref>(?:https?:\/\/|mailto:)[^<>\s]+)>/g;

export function parseInlineMarkdown(text: string): InlineToken[] {
  if (!text) return [];
  const tokens: InlineToken[] = [];
  INLINE_PATTERN.lastIndex = 0;
  let lastIndex = 0;

  for (const match of text.matchAll(INLINE_PATTERN)) {
    const index = match.index ?? 0;
    if (index > lastIndex) pushText(tokens, text.slice(lastIndex, index));
    pushMatch(tokens, match);
    lastIndex = index + match[0].length;
  }
  if (lastIndex < text.length) pushText(tokens, text.slice(lastIndex));
  return tokens;
}

export function parseInlineMarkdownLines(lines: string[]): InlineToken[] {
  const tokens: InlineToken[] = [];
  lines.forEach((line, index) => {
    const text = line.replace(/(?:\\| {2,})$/, "");
    tokens.push(...parseInlineMarkdown(index === 0 ? text.trimStart() : text.trim()));
    if (index < lines.length - 1) tokens.push(makeInlineToken("break", ""));
  });
  return tokens;
}

function pushMatch(tokens: InlineToken[], match: RegExpMatchArray) {
  const groups = match.groups ?? {};
  if (groups.code !== undefined) return tokens.push(makeInlineToken("code", groups.code));

  const strong = groups.starStrong ?? groups.underscoreStrong;
  if (strong !== undefined) return tokens.push(makeInlineToken("strong", strong));
  const emphasis = groups.underscoreEm ?? groups.starEm;
  if (emphasis !== undefined) return tokens.push(makeInlineToken("em", emphasis));

  const linkText = groups.linkText ?? groups.angleHref;
  const rawHref = groups.linkHref ?? groups.angleHref;
  if (linkText !== undefined && rawHref !== undefined) {
    const href = normalizeExternalHref(rawHref);
    tokens.push(href
      ? makeInlineToken("link", linkText, href)
      : makeInlineToken("text", match[0]));
  }
}

function pushText(tokens: InlineToken[], text: string) {
  if (!text) return;
  const linkPattern = /\bhttps?:\/\/[^\s<]+|\bmailto:[^\s<]+/g;
  let lastIndex = 0;
  for (const match of text.matchAll(linkPattern)) {
    const index = match.index ?? 0;
    if (index > lastIndex) tokens.push(makeInlineToken("text", text.slice(lastIndex, index)));
    const { href, suffix } = splitAutoLink(match[0]);
    const normalized = normalizeExternalHref(href);
    tokens.push(normalized
      ? makeInlineToken("link", href, normalized)
      : makeInlineToken("text", href));
    if (suffix) tokens.push(makeInlineToken("text", suffix));
    lastIndex = index + match[0].length;
  }
  if (lastIndex < text.length) tokens.push(makeInlineToken("text", text.slice(lastIndex)));
}

function splitAutoLink(raw: string) {
  let href = raw;
  let suffix = "";
  while (/[.,!?;:]$/.test(href) || (href.endsWith(")") && !hasBalancedParens(href))) {
    suffix = href.slice(-1) + suffix;
    href = href.slice(0, -1);
  }
  return { href, suffix };
}

function hasBalancedParens(text: string) {
  return (text.match(/\)/g) ?? []).length <= (text.match(/\(/g) ?? []).length;
}

export function normalizeExternalHref(href: string): string | null {
  const trimmed = href.trim();
  return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : null;
}
