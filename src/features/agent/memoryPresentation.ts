import type { MemoryRecord } from "./types";

const layerLabels: Record<string, string> = {
  Overview: "项目概览",
  Stage: "当前阶段",
  Structure: "模型结构",
  Decisions: "关键决策",
  Summary: "经验摘要",
  Technique: "技术方法",
  Caveats: "注意事项",
};

export function memoryLayerLabel(name: string): string {
  return layerLabels[name] ?? name;
}

export function formatMemoryTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString([], {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function matchesMemory(memory: MemoryRecord, query: string): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  return [
    memory.title,
    memory.projectName ?? "",
    ...memory.layers.map((layer) => layer.content),
  ].some((value) => value.toLocaleLowerCase().includes(normalized));
}
