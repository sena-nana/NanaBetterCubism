import { parseDelimited } from "../../../utils/delimited";
import type { ParameterInputRow } from "../types";
import { makeRow } from "./idTemplate";

export function parsePastedRows(value: string): ParameterInputRow[] {
  const normalized = value.trim();
  if (!normalized) return [];
  const records = parseDelimited(normalized, normalized.includes("\t") ? "\t" : ",");
  const first = records[0]?.map((cell) => cell.trim().toLowerCase()) ?? [];
  const hasHeader = first[0] === "名称" || first[0] === "name";
  return records
    .slice(hasHeader ? 1 : 0)
    .filter((record) => record.some((cell) => cell.trim()))
    .map(([name = "", key = "", side = ""]) => makeRow({
      name: name.trim(),
      key: key.trim(),
      side: side.trim(),
    }));
}
