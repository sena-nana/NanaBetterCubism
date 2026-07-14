<script setup lang="ts">
import { Dropdown, UiInput } from "@lilia/ui";
import { computed } from "vue";
import type { ParameterGroupSummary, ParameterInputRow, RowGroupSelection } from "../types";

const props = defineProps<{ row: ParameterInputRow; groups: ParameterGroupSummary[] }>();

const booleanOptions = [
  { value: "inherit", label: "继承默认" },
  { value: "true", label: "开启" },
  { value: "false", label: "关闭" },
];
const groupOptions = computed(() => [
  { value: "inherit", label: "继承默认" },
  { value: "root", label: "根级" },
  ...props.groups.map((group) => ({ value: `existing:${group.id}`, label: group.name })),
]);

function setNumber(field: "min" | "default" | "max", value: string) {
  if (value === "") delete props.row.overrides[field];
  else props.row.overrides[field] = Number(value);
}

function booleanValue(value: boolean | undefined) {
  return value === undefined ? "inherit" : String(value);
}

function setBoolean(field: "isBlendShape" | "isRepeat", value: string) {
  if (value === "inherit") delete props.row.overrides[field];
  else props.row.overrides[field] = value === "true";
}

function groupValue(group: RowGroupSelection | undefined) {
  if (!group) return "inherit";
  return group.kind === "existing" ? `existing:${group.id}` : "root";
}

function setGroup(value: string) {
  if (value === "inherit") delete props.row.overrides.group;
  else if (value === "root") props.row.overrides.group = { kind: "root" };
  else props.row.overrides.group = { kind: "existing", id: value.slice("existing:".length) };
}
</script>

<template>
  <div class="override-row">
    <label v-for="field in (['min', 'default', 'max'] as const)" :key="field">
      <span>{{ { min: "最小值", default: "默认值", max: "最大值" }[field] }}</span>
      <UiInput
        :model-value="row.overrides[field] ?? ''"
        type="number"
        @update:model-value="setNumber(field, $event)"
      />
    </label>
    <label>
      <span>Blend Shape</span>
      <Dropdown
        :model-value="booleanValue(row.overrides.isBlendShape)"
        :options="booleanOptions"
        block
        placement="bottom"
        @update:model-value="setBoolean('isBlendShape', $event)"
      />
    </label>
    <label>
      <span>Repeat</span>
      <Dropdown
        :model-value="booleanValue(row.overrides.isRepeat)"
        :options="booleanOptions"
        block
        placement="bottom"
        @update:model-value="setBoolean('isRepeat', $event)"
      />
    </label>
    <label>
      <span>参数组</span>
      <Dropdown
        :model-value="groupValue(row.overrides.group)"
        :options="groupOptions"
        block
        placement="bottom"
        @update:model-value="setGroup"
      />
    </label>
  </div>
</template>

<style scoped>
.override-row { display: grid; grid-template-columns: repeat(6, minmax(110px, 1fr)); gap: 8px; padding: 9px 8px 10px; background: var(--bg-subtle); border-top: 1px solid var(--border-soft); }
.override-row label { display: flex; flex-direction: column; gap: 4px; }
.override-row label > span { color: var(--text-muted); font-size: 11px; font-weight: 600; }
@media (max-width: 1050px) { .override-row { grid-template-columns: repeat(3, minmax(110px, 1fr)); } }
</style>
