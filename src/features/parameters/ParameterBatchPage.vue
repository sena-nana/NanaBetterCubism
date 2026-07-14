<script setup lang="ts">
import Plus from "@lucide/vue/dist/esm/icons/plus.mjs";
import Trash2 from "@lucide/vue/dist/esm/icons/trash-2.mjs";
import Upload from "@lucide/vue/dist/esm/icons/upload.mjs";
import {
  ConfirmDialog,
  Dropdown,
  UiButton,
  UiCard,
  UiEmptyState,
  UiIconButton,
  UiInput,
  UiSwitch,
} from "@lilia/ui";
import { computed, defineAsyncComponent, onMounted, reactive, ref, watch } from "vue";
import EditorConnectionCard from "../editor/EditorConnectionCard.vue";
import { useParameterPresets } from "./composables/useParameterPresets";
import { useEditorStore } from "./editorStore";
import {
  createLocalPreview,
  makeRow,
  previewRowFromLocal,
} from "./utils/idTemplate";
import type {
  BatchGroupSelection,
  ParameterBatchInput,
  ParameterBatchPreview,
  ParameterDefaults,
  ParameterInputRow,
} from "./types";

const ParameterPastePanel = defineAsyncComponent(() => import("./components/ParameterPastePanel.vue"));
const ParameterRowOverrides = defineAsyncComponent(() => import("./components/ParameterRowOverrides.vue"));
const ParameterOperationCard = defineAsyncComponent(() => import("./components/ParameterOperationCard.vue"));

const editor = useEditorStore();
const {
  idTemplate,
  selectedPresetId,
  presetName,
  presetOptions,
  selectedPreset,
  choosePreset,
  savePreset,
  renamePreset,
  deletePreset,
} = useParameterPresets();
const defaults = reactive<ParameterDefaults>({
  min: -1,
  default: 0,
  max: 1,
  isBlendShape: false,
  isRepeat: false,
  group: { kind: "root" },
});
const rows = ref<ParameterInputRow[]>([makeRow()]);
const advancedRows = ref(new Set<string>());
const pasteOpen = ref(false);
const backendPreview = ref<ParameterBatchPreview | null>(null);
const confirmOpen = ref(false);

const groupOptions = computed(() => [
  { value: "root", label: "根级" },
  ...editor.state.snapshot.groups.map((group) => ({
    value: `existing:${group.id}`,
    label: `${group.name} · ${group.id}`,
  })),
  { value: "new", label: "新建参数组" },
]);
const defaultGroupValue = computed({
  get: () => groupValue(defaults.group),
  set: (value: string) => {
    defaults.group = parseBatchGroup(value, defaults.group);
  },
});
const localPreview = computed(() => createLocalPreview(idTemplate, rows.value, defaults));
const localPreviewRows = computed(() =>
  rows.value.map((row) =>
    previewRowFromLocal(row, localPreview.value.ids.get(row.clientId) ?? "", defaults),
  ),
);
const displayedPreview = computed(() => backendPreview.value?.rows ?? localPreviewRows.value);
const displayedErrors = computed(() => backendPreview.value?.errors ?? localPreview.value.errors);
const canRequestPreview = computed(
  () => editor.canCreate.value && localPreview.value.errors.length === 0 && !editor.operationActive.value,
);
const canExecute = computed(
  () => Boolean(backendPreview.value?.canExecute && backendPreview.value.previewId) && !editor.operationActive.value,
);
const blankOnly = computed(() => rows.value.length === 1 && isBlankRow(rows.value[0]));
const confirmMessage = computed(() => {
  const preview = backendPreview.value;
  if (!preview?.rows.length) return "没有可执行的参数。";
  const group = preview.newGroup ? `，并新建参数组 ${preview.newGroup.name}` : "";
  const ids = preview.rows.map((row) => row.id);
  const visibleIds = ids.slice(0, 10).join("、");
  const remaining = ids.length > 10 ? `，另有 ${ids.length - 10} 个` : "";
  return `将在 ${preview.modelLabel} 中创建 ${preview.rows.length} 个参数${group}。最终 ID：${visibleIds}${remaining}。`;
});

watch(
  [idTemplate, defaults, rows],
  () => {
    backendPreview.value = null;
    confirmOpen.value = false;
  },
  { deep: true },
);

onMounted(() => {
  void editor.initialize();
});

function addRow() {
  if (rows.value.length >= 200) return;
  rows.value.push(makeRow());
}

function removeRow(clientId: string) {
  rows.value = rows.value.filter((row) => row.clientId !== clientId);
}

function toggleAdvanced(clientId: string) {
  const next = new Set(advancedRows.value);
  if (next.has(clientId)) next.delete(clientId);
  else next.add(clientId);
  advancedRows.value = next;
}

function importPastedRows(imported: ParameterInputRow[]) {
  rows.value = [...(blankOnly.value ? [] : rows.value), ...imported];
  pasteOpen.value = false;
}

async function requestBackendPreview() {
  if (!canRequestPreview.value) return;
  backendPreview.value = await editor.preview(batchInput());
}

async function executePreview() {
  const previewId = backendPreview.value?.previewId;
  if (!previewId) return;
  confirmOpen.value = false;
  await editor.execute(previewId);
  backendPreview.value = null;
}

function batchInput(): ParameterBatchInput {
  return {
    idTemplate: {
      ...idTemplate,
      startIndex: Number(idTemplate.startIndex),
      indexWidth: Number(idTemplate.indexWidth),
    },
    defaults: {
      ...structuredClone(defaults),
      min: Number(defaults.min),
      default: Number(defaults.default),
      max: Number(defaults.max),
    },
    rows: structuredClone(rows.value),
  };
}

function groupValue(group: BatchGroupSelection) {
  if (group.kind === "existing") return `existing:${group.id}`;
  return group.kind;
}

function parseBatchGroup(value: string, previous: BatchGroupSelection): BatchGroupSelection {
  if (value === "root") return { kind: "root" };
  if (value === "new") {
    return previous.kind === "new" ? previous : { kind: "new", id: "ParamGroupCustom", name: "自定义" };
  }
  return { kind: "existing", id: value.slice("existing:".length) };
}

function isBlankRow(row: ParameterInputRow) {
  return !row.name && !row.key && !row.side;
}
</script>

<template>
  <section class="parameter-page" data-agent-id="parameters.page">
    <div class="page-header" data-agent-id="parameters.header">
      <div>
        <h1>批量生成参数</h1>
        <p>为当前 Cubism 模型生成统一、可复用的参数 ID。</p>
      </div>
      <div class="connection-state" :class="`is-${editor.state.snapshot.state}`">
        <span class="connection-state__dot"></span>
        <span>{{ editor.state.snapshot.message }}</span>
      </div>
    </div>

    <EditorConnectionCard
      agent-id-prefix="parameters.connection"
      :disconnect-disabled="editor.operationActive.value"
    />
    <p v-if="editor.state.error" class="message message--error" role="alert">
      {{ editor.state.error.message }}
    </p>

    <div class="config-grid">
      <UiCard title="ID 格式" agent-id="parameters.id-format">
        <div class="preset-row">
          <Dropdown
            :model-value="selectedPresetId"
            :options="presetOptions"
            block
            size="large"
            menu-label="ID 格式预设"
            agent-id="parameters.preset.select"
            @update:model-value="choosePreset"
          />
          <UiInput v-model="presetName" placeholder="预设名称" agent-id="parameters.preset.name" />
          <UiButton size="sm" agent-id="parameters.preset.save" @click="savePreset">另存</UiButton>
          <UiButton
            size="sm"
            :disabled="selectedPreset?.builtIn || !presetName.trim()"
            agent-id="parameters.preset.rename"
            @click="renamePreset"
          >
            重命名
          </UiButton>
          <UiIconButton
            :icon="Trash2"
            label="删除预设"
            title="删除预设"
            :disabled="selectedPreset?.builtIn"
            agent-id="parameters.preset.delete"
            @click="deletePreset"
          />
        </div>
        <div class="field-grid">
          <div class="field field--wide">
            <label>ID 模板</label>
            <UiInput v-model="idTemplate.template" agent-id="parameters.template.pattern" />
          </div>
          <div class="field">
            <label>前缀</label>
            <UiInput v-model="idTemplate.prefix" agent-id="parameters.template.prefix" />
          </div>
          <div class="field">
            <label>后缀</label>
            <UiInput v-model="idTemplate.suffix" agent-id="parameters.template.suffix" />
          </div>
          <div class="field">
            <label>起始编号</label>
            <UiInput v-model="idTemplate.startIndex" type="number" agent-id="parameters.template.start-index" />
          </div>
          <div class="field">
            <label>补零位数</label>
            <UiInput v-model="idTemplate.indexWidth" type="number" agent-id="parameters.template.index-width" />
          </div>
        </div>
        <p class="hint">可用令牌：{prefix}、{key}、{side}、{index}、{suffix}</p>
      </UiCard>

      <UiCard title="批量默认" agent-id="parameters.defaults">
        <div class="field-grid">
          <div class="field">
            <label>最小值</label>
            <UiInput v-model="defaults.min" type="number" agent-id="parameters.defaults.min" />
          </div>
          <div class="field">
            <label>默认值</label>
            <UiInput v-model="defaults.default" type="number" agent-id="parameters.defaults.default" />
          </div>
          <div class="field">
            <label>最大值</label>
            <UiInput v-model="defaults.max" type="number" agent-id="parameters.defaults.max" />
          </div>
          <div class="field field--wide">
            <label>参数组</label>
            <Dropdown
              v-model="defaultGroupValue"
              :options="groupOptions"
              block
              size="large"
              menu-label="批量默认参数组"
              agent-id="parameters.defaults.group"
            />
          </div>
          <template v-if="defaults.group.kind === 'new'">
            <div class="field">
              <label>新组名称</label>
              <UiInput v-model="defaults.group.name" agent-id="parameters.defaults.new-group-name" />
            </div>
            <div class="field">
              <label>新组 ID</label>
              <UiInput v-model="defaults.group.id" agent-id="parameters.defaults.new-group-id" />
            </div>
          </template>
        </div>
        <div class="switch-row">
          <UiSwitch v-model="defaults.isBlendShape" label="Blend Shape" agent-id="parameters.defaults.blend-shape" />
          <UiSwitch v-model="defaults.isRepeat" label="Repeat" agent-id="parameters.defaults.repeat" />
        </div>
      </UiCard>
    </div>

    <UiCard title="参数列表" agent-id="parameters.rows">
      <template #title>
        <span>参数列表</span>
        <span class="count">{{ rows.length }} / 200</span>
      </template>
      <div class="toolbar">
        <UiButton :icon="Plus" size="sm" :disabled="rows.length >= 200" agent-id="parameters.rows.add" @click="addRow">
          添加一行
        </UiButton>
        <UiButton :icon="Upload" size="sm" agent-id="parameters.rows.open-paste" @click="pasteOpen = !pasteOpen">
          粘贴批量数据
        </UiButton>
        <span class="hint">列顺序：名称、ID 段、方位（可选）</span>
      </div>
      <ParameterPastePanel
        v-if="pasteOpen"
        :current-count="rows.length"
        :replace-blank-row="blankOnly"
        @close="pasteOpen = false"
        @import="importPastedRows"
      />

      <div v-if="rows.length" class="parameter-table" role="table" aria-label="待创建参数">
        <div class="parameter-row parameter-row--head" role="row">
          <span>名称</span><span>ID 段</span><span>方位</span><span>生成 ID</span><span>操作</span>
        </div>
        <div
          v-for="row in rows"
          :key="row.clientId"
          class="parameter-row-wrap"
          :data-agent-id="`parameters.row.${row.clientId}`"
        >
          <div class="parameter-row" role="row">
            <UiInput v-model="row.name" :agent-id="`parameters.row.${row.clientId}.name`" />
            <UiInput v-model="row.key" :agent-id="`parameters.row.${row.clientId}.key`" />
            <UiInput v-model="row.side" :agent-id="`parameters.row.${row.clientId}.side`" />
            <code :title="localPreview.ids.get(row.clientId)">{{ localPreview.ids.get(row.clientId) || "-" }}</code>
            <div class="row-actions">
              <UiButton size="sm" :agent-id="`parameters.row.${row.clientId}.advanced`" @click="toggleAdvanced(row.clientId)">
                {{ advancedRows.has(row.clientId) ? "收起" : "覆盖" }}
              </UiButton>
              <UiIconButton
                :icon="Trash2"
                label="删除参数"
                title="删除参数"
                :agent-id="`parameters.row.${row.clientId}.delete`"
                @click="removeRow(row.clientId)"
              />
            </div>
          </div>
          <ParameterRowOverrides
            v-if="advancedRows.has(row.clientId)"
            :row="row"
            :groups="editor.state.snapshot.groups"
          />
        </div>
      </div>
      <UiEmptyState v-else title="没有参数" message="添加一行或粘贴批量数据。" agent-id="parameters.rows.empty" />
    </UiCard>

    <UiCard title="创建预览" agent-id="parameters.preview">
      <div v-if="displayedErrors.length" class="issue-list" role="alert">
        <p v-for="(error, index) in displayedErrors.slice(0, 8)" :key="`${error.code}-${index}`">
          {{ error.message }}
        </p>
        <p v-if="displayedErrors.length > 8">另有 {{ displayedErrors.length - 8 }} 项错误。</p>
      </div>
      <div v-else class="preview-list">
        <div v-for="row in displayedPreview" :key="row.clientId" class="preview-item">
          <div><strong>{{ row.name || "未命名" }}</strong><code>{{ row.id }}</code></div>
          <span>{{ row.min }} / {{ row.default }} / {{ row.max }} · {{ row.groupLabel }}</span>
        </div>
      </div>
      <div class="preview-actions">
        <span class="hint">执行前会重新读取当前模型并检查所有 ID。</span>
        <UiButton
          :disabled="!canRequestPreview"
          :busy="editor.state.busy"
          agent-id="parameters.preview.validate"
          @click="requestBackendPreview"
        >
          校验当前模型
        </UiButton>
        <UiButton
          variant="primary"
          :disabled="!canExecute"
          agent-id="parameters.preview.execute"
          @click="confirmOpen = true"
        >
          创建参数
        </UiButton>
      </div>
    </UiCard>

    <ParameterOperationCard
      v-if="editor.operationActive.value || editor.state.finished"
      :active="editor.operationActive.value"
      :state="editor.state.snapshot.state"
      :progress="editor.state.progress"
      :finished="editor.state.finished"
      @cancel="editor.cancel"
    />

    <ConfirmDialog
      :open="confirmOpen"
      title="确认创建参数"
      :message="confirmMessage"
      confirm-text="开始创建"
      :busy="editor.state.busy"
      busy-text="正在启动…"
      @confirm="executePreview"
      @cancel="confirmOpen = false"
    />
  </section>
</template>

<style scoped>
.parameter-page { display: flex; flex-direction: column; gap: 12px; }
.page-header { align-items: flex-start; }
.connection-state { display: inline-flex; align-items: center; gap: 7px; max-width: 440px; color: var(--text-muted); font-size: 12px; text-align: right; }
.connection-state__dot { width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto; background: var(--text-faint); }
.connection-state.is-ready .connection-state__dot { background: var(--ok); }
.connection-state.is-failed .connection-state__dot { background: var(--err); }
.connection-state.is-connecting .connection-state__dot,
.connection-state.is-awaiting_access .connection-state__dot,
.connection-state.is-awaiting_edit_permission .connection-state__dot,
.connection-state.is-editing .connection-state__dot,
.connection-state.is-cancelling .connection-state__dot,
.connection-state.is-incompatible .connection-state__dot { background: var(--warn); }
.config-grid { display: grid; grid-template-columns: minmax(0, 1.35fr) minmax(320px, 0.65fr); gap: 12px; align-items: start; }
.preset-row, .toolbar, .switch-row, .preview-actions { display: flex; align-items: center; gap: 8px; }
.preset-row { margin-bottom: 12px; }
.preset-row > :first-child { flex: 1 1 220px; }
.preset-row :deep(.ui-input) { width: 150px; }
.field-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 9px; }
.field { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
.field--wide { grid-column: span 2; }
.field label, .override-row label > span { color: var(--text-muted); font-size: 11px; font-weight: 600; }
.hint { margin: 8px 0 0; color: var(--text-faint); font-size: 11px; }
.switch-row { margin-top: 12px; }
.toolbar { margin-bottom: 10px; }
.toolbar .hint, .preview-actions .hint { margin: 0; }
.count { margin-left: 6px; color: var(--text-faint); font-size: 11px; font-weight: 500; }
.parameter-table { overflow: auto; border: 1px solid var(--border-soft); border-radius: var(--radius-md); }
.parameter-row { display: grid; grid-template-columns: minmax(150px, 1.2fr) minmax(120px, 1fr) 80px minmax(170px, 1.25fr) 116px; gap: 7px; align-items: center; padding: 7px 8px; }
.parameter-row--head { position: sticky; top: 0; z-index: 1; min-height: 30px; background: var(--bg-subtle); color: var(--text-muted); font-size: 11px; font-weight: 600; }
.parameter-row-wrap + .parameter-row-wrap { border-top: 1px solid var(--border-soft); }
.parameter-row code, .preview-item code { overflow: hidden; color: var(--accent); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
.row-actions { display: flex; justify-content: flex-end; gap: 3px; }
.issue-list { padding: 8px 10px; border-radius: var(--radius-md); background: var(--err-soft); color: var(--err); font-size: 12px; }
.issue-list p { margin: 2px 0; }
.preview-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; max-height: 240px; overflow: auto; }
.preview-item { display: flex; justify-content: space-between; gap: 12px; padding: 7px 8px; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); }
.preview-item > div { display: flex; gap: 8px; min-width: 0; }
.preview-item strong { overflow: hidden; font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.preview-item > span { color: var(--text-faint); font-size: 11px; white-space: nowrap; }
.preview-actions { justify-content: flex-end; margin-top: 10px; }
.preview-actions .hint { margin-right: auto; }
.message { margin: 8px 0 0; font-size: 12px; }
.message--error { color: var(--err); }
.message--warning { color: var(--warn); }
.message--success { color: var(--ok); }
@media (max-width: 1050px) {
  .config-grid { grid-template-columns: 1fr; }
}
@media (prefers-reduced-motion: reduce) {
  * { transition: none !important; }
}
</style>
