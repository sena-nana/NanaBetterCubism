import type {
  IdTemplateConfig,
  ParameterInputRow,
  ParameterPreviewRow,
  ValidationIssue,
} from "../types";

const TOKENS = new Set(["prefix", "key", "side", "index", "suffix"]);
let rowCounter = 0;

export function makeRow(partial: Partial<ParameterInputRow> = {}): ParameterInputRow {
  rowCounter += 1;
  return {
    clientId: partial.clientId ?? `row-${Date.now()}-${rowCounter}`,
    name: partial.name ?? "",
    key: partial.key ?? "",
    side: partial.side ?? "",
    overrides: partial.overrides ?? {},
  };
}

export function expandIdTemplate(
  config: IdTemplateConfig,
  row: Pick<ParameterInputRow, "key" | "side">,
  position: number,
) {
  const index = String(Number(config.startIndex) + position).padStart(Number(config.indexWidth), "0");
  return [
    ["prefix", config.prefix],
    ["key", row.key.trim()],
    ["side", row.side.trim()],
    ["index", index],
    ["suffix", config.suffix],
  ].reduce((value, [token, replacement]) => value.split(`{${token}}`).join(replacement), config.template);
}

export function validateTemplate(template: string) {
  if (!template) return "ID 模板不能为空。";
  for (const match of template.matchAll(/\{([^}]*)\}/g)) {
    if (!TOKENS.has(match[1])) return `不支持模板令牌 {${match[1]}}。`;
  }
  const stripped = template.replace(/\{[^}]*\}/g, "");
  return stripped.includes("{") || stripped.includes("}") ? "ID 模板包含未闭合的令牌。" : null;
}

export function validateCubismId(id: string) {
  if (id.length < 1 || id.length > 63) return "ID 长度必须在 1 到 63 个字符之间。";
  if (/^[0-9]/.test(id)) return "ID 不能以数字开头。";
  if (!/^[A-Za-z0-9_]+$/.test(id)) return "ID 只能包含字母、数字和下划线。";
  return null;
}

export function createLocalPreview(
  config: IdTemplateConfig,
  rows: ParameterInputRow[],
  defaults: { min: number; default: number; max: number } = { min: -1, default: 0, max: 1 },
): { ids: Map<string, string>; errors: ValidationIssue[] } {
  const ids = new Map<string, string>();
  const errors: ValidationIssue[] = [];
  const templateError = validateTemplate(config.template);
  if (templateError) errors.push(issue("invalid_template", templateError, null, "template"));
  if (!Number.isInteger(Number(config.startIndex)) || Number(config.startIndex) < 0) {
    errors.push(issue("invalid_start_index", "起始编号必须是非负整数。", null, "startIndex"));
  }
  if (!Number.isInteger(Number(config.indexWidth)) || Number(config.indexWidth) < 1 || Number(config.indexWidth) > 6) {
    errors.push(issue("invalid_index_width", "编号补零位数必须在 1 到 6 之间。", null, "indexWidth"));
  }
  const seen = new Set<string>();
  rows.forEach((row, index) => {
    const id = templateError ? "" : expandIdTemplate(config, row, index);
    ids.set(row.clientId, id);
    if (!row.name.trim()) errors.push(issue("empty_name", "参数名称不能为空。", row.clientId, "name"));
    const idError = validateCubismId(id);
    if (idError) errors.push(issue("invalid_id", idError, row.clientId, "id"));
    if (seen.has(id)) errors.push(issue("duplicate_id", `本批次重复生成了 ${id}。`, row.clientId, "id"));
    seen.add(id);

    const min = Number(row.overrides.min ?? defaults.min);
    const defaultValue = Number(row.overrides.default ?? defaults.default);
    const max = Number(row.overrides.max ?? defaults.max);
    if (![min, defaultValue, max].every(Number.isFinite) || min > defaultValue || defaultValue > max) {
      errors.push(issue("invalid_range", "参数范围必须是有限数字，并满足最小值 ≤ 默认值 ≤ 最大值。", row.clientId, "range"));
    }
  });
  if (!rows.length) errors.push(issue("empty_batch", "至少需要一个参数。", null, null));
  if (rows.length > 200) errors.push(issue("batch_too_large", "单批最多创建 200 个参数。", null, null));
  return { ids, errors };
}

export function previewRowFromLocal(
  row: ParameterInputRow,
  id: string,
  defaults: { min: number; default: number; max: number; isBlendShape: boolean; isRepeat: boolean },
): ParameterPreviewRow {
  return {
    clientId: row.clientId,
    name: row.name,
    id,
    groupId: null,
    groupLabel: "待后端校验",
    min: row.overrides.min ?? defaults.min,
    default: row.overrides.default ?? defaults.default,
    max: row.overrides.max ?? defaults.max,
    isBlendShape: row.overrides.isBlendShape ?? defaults.isBlendShape,
    isRepeat: row.overrides.isRepeat ?? defaults.isRepeat,
  };
}

export function copyTemplate(value: IdTemplateConfig): IdTemplateConfig {
  return {
    template: value.template,
    prefix: value.prefix,
    suffix: value.suffix,
    startIndex: value.startIndex,
    indexWidth: value.indexWidth,
  };
}

function issue(code: string, message: string, rowId: string | null, field: string | null): ValidationIssue {
  return { code, message, rowId, field };
}
