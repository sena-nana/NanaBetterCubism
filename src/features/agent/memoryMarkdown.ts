export type MemoryScope = "project" | "global";

/** Extract Overview / Summary, or fall back to plain text. */
export function extractMemoryOverview(scope: MemoryScope, body: string): string {
  const index = scope === "project" ? "Overview" : "Summary";
  const marker = `## ${index}`;
  const start = body.indexOf(marker);
  if (start < 0) {
    return body.trim();
  }
  const after = body.slice(start + marker.length);
  const next = after.search(/\n##\s/);
  const section = (next < 0 ? after : after.slice(0, next)).trim();
  return section || body.trim();
}
