<script setup lang="ts">
import Search from "@lucide/vue/dist/esm/icons/search.mjs";
import { UiButton, UiCard, UiEmptyState } from "@lilia/ui";
import { computed, onMounted, ref } from "vue";
import EditorConnectionCard from "../editor/EditorConnectionCard.vue";
import { normalizeCommandError } from "../parameters/bridge";
import { useEditorStore } from "../parameters/editorStore";
import type { DomainCommandError } from "../parameters/types";
import { findSelectedPartParameters } from "./bridge";
import type { PartParameterQueryResult } from "./types";

const editor = useEditorStore();
const result = ref<PartParameterQueryResult | null>(null);
const error = ref<DomainCommandError | null>(null);
const busy = ref(false);
const expandedParameters = ref(new Set<string>());

const canQuery = computed(() => editor.canFindPartParameters.value && !busy.value);
const sourcePartNames = computed(() =>
  new Map(result.value?.selectedParts.map((part) => [part.id, part.name]) ?? []),
);

onMounted(() => {
  void editor.initialize();
});

async function query() {
  if (!canQuery.value) return;
  busy.value = true;
  error.value = null;
  result.value = null;
  expandedParameters.value = new Set();
  try {
    result.value = await findSelectedPartParameters();
  } catch (value) {
    error.value = normalizeCommandError(value);
  } finally {
    busy.value = false;
  }
}

function toggleParameter(parameterId: string) {
  const next = new Set(expandedParameters.value);
  if (next.has(parameterId)) next.delete(parameterId);
  else next.add(parameterId);
  expandedParameters.value = next;
}

function keyValues(values: number[]) {
  return values.length ? values.join("、") : "无";
}

function sourceParts(ids: string[]) {
  return ids.map((id) => sourcePartNames.value.get(id) ?? id).join("、");
}
</script>

<template>
  <section class="part-parameters-page" data-agent-id="part-parameters.page">
    <div class="page-header" data-agent-id="part-parameters.header">
      <div>
        <h1>部件关联参数</h1>
        <p>查找 Cubism Editor 当前选中 Part 及其完整子树使用的全部参数。</p>
      </div>
      <div class="connection-state" :class="`is-${editor.state.snapshot.state}`">
        <span class="connection-state__dot"></span>
        <span>{{ editor.state.snapshot.message }}</span>
      </div>
    </div>

    <EditorConnectionCard agent-id-prefix="part-parameters.connection" :disconnect-disabled="busy" />

    <UiCard title="查询当前选择" agent-id="part-parameters.query">
      <div class="query-row">
        <div>
          <p>在 Cubism Editor 中选择一个或多个 Part，然后读取其自身、嵌套 Part 和全部后代对象。</p>
          <p class="hint">选择或模型变化后需要重新查询；非 Part 选择会被忽略。</p>
        </div>
        <UiButton
          variant="primary"
          :icon="Search"
          :disabled="!canQuery"
          :busy="busy"
          agent-id="part-parameters.query.run"
          @click="query"
        >
          查找关联参数
        </UiButton>
      </div>
      <p
        v-if="editor.state.snapshot.state === 'ready' && !editor.state.snapshot.capabilities.findPartParameters"
        class="message message--warning"
      >
        当前 Editor 会话未确认部件关联查询能力。
      </p>
    </UiCard>

    <UiCard title="查询结果" agent-id="part-parameters.result">
      <div v-if="busy" class="status-panel" role="status">正在读取所选 Part 的对象和参数键…</div>
      <div v-else-if="error" class="status-panel status-panel--error" role="alert">
        <strong>查询失败</strong>
        <span>{{ error.message }}</span>
      </div>
      <UiEmptyState
        v-else-if="!result"
        title="尚未查询"
        message="在 Editor 中选择 Part 后运行查询。"
        agent-id="part-parameters.result.initial"
      />
      <template v-else>
        <div class="result-summary">
          <div>
            <span class="summary-label">已选 Part</span>
            <strong>{{ result.selectedParts.length }}</strong>
          </div>
          <div>
            <span class="summary-label">扫描对象</span>
            <strong>{{ result.scannedObjectCount }}</strong>
          </div>
          <div>
            <span class="summary-label">关联参数</span>
            <strong>{{ result.parameters.length }}</strong>
          </div>
          <span class="model-label">{{ result.modelLabel }}</span>
        </div>
        <div class="selected-parts">
          <span v-for="part in result.selectedParts" :key="part.id" :title="part.id">
            {{ part.name }} · {{ part.id }}
          </span>
        </div>
        <p v-if="result.ignoredSelectionCount" class="message message--warning">
          已忽略 {{ result.ignoredSelectionCount }} 个非 Part 选择。
        </p>
        <UiEmptyState
          v-if="!result.parameters.length"
          title="没有关联参数"
          message="所选 Part 的递归子树中没有参数键。"
          agent-id="part-parameters.result.empty"
        />
        <div v-else class="parameter-results">
          <article
            v-for="parameter in result.parameters"
            :key="parameter.id"
            class="parameter-result"
            :data-agent-id="`part-parameters.parameter.${parameter.id}`"
          >
            <div class="parameter-result__main">
              <div class="parameter-identity">
                <strong>{{ parameter.name }}</strong>
                <code>{{ parameter.id }}</code>
              </div>
              <div class="parameter-meta">
                <span>{{ parameter.group ? `${parameter.group.name} · ${parameter.group.id}` : "根级" }}</span>
                <span>键值 {{ keyValues(parameter.keyValues) }}</span>
                <span>{{ parameter.objects.length }} 个对象</span>
              </div>
              <UiButton
                size="sm"
                :agent-id="`part-parameters.parameter.${parameter.id}.toggle`"
                @click="toggleParameter(parameter.id)"
              >
                {{ expandedParameters.has(parameter.id) ? "收起对象" : "查看对象" }}
              </UiButton>
            </div>
            <div v-if="expandedParameters.has(parameter.id)" class="object-list">
              <div
                v-for="object in parameter.objects"
                :key="object.id"
                class="object-row"
                :data-agent-id="`part-parameters.parameter.${parameter.id}.object.${object.id}`"
              >
                <div>
                  <strong>{{ object.name }}</strong>
                  <code>{{ object.id }}</code>
                </div>
                <span>{{ object.objectType }}</span>
                <span>键值 {{ keyValues(object.keyValues) }}</span>
                <span>来源 {{ sourceParts(object.sourcePartIds) }}</span>
              </div>
            </div>
          </article>
        </div>
      </template>
    </UiCard>
  </section>
</template>

<style scoped>
.part-parameters-page { display: flex; flex-direction: column; gap: 12px; }
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
.query-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.query-row p { margin: 0; color: var(--text-muted); font-size: 12px; }
.hint { margin-top: 5px !important; color: var(--text-faint) !important; font-size: 11px !important; }
.message { margin: 8px 0 0; font-size: 12px; }
.message--warning { color: var(--warn); }
.status-panel { display: flex; flex-direction: column; gap: 3px; min-height: 68px; justify-content: center; color: var(--text-muted); font-size: 12px; }
.status-panel--error { padding: 10px; border-radius: var(--radius-md); background: var(--err-soft); color: var(--err); }
.result-summary { display: flex; align-items: center; gap: 24px; padding-bottom: 10px; border-bottom: 1px solid var(--border-soft); }
.result-summary > div { display: flex; flex-direction: column; gap: 2px; }
.summary-label, .model-label { color: var(--text-faint); font-size: 11px; }
.model-label { margin-left: auto; }
.selected-parts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
.selected-parts span { padding: 3px 7px; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); color: var(--text-muted); font-size: 11px; }
.parameter-results { display: flex; flex-direction: column; gap: 7px; margin-top: 10px; }
.parameter-result { border: 1px solid var(--border-soft); border-radius: var(--radius-md); overflow: hidden; }
.parameter-result__main { display: grid; grid-template-columns: minmax(180px, 1fr) minmax(280px, 1.5fr) auto; gap: 12px; align-items: center; padding: 9px 10px; }
.parameter-identity { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.parameter-identity strong { overflow: hidden; font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
code { color: var(--accent); font-size: 11px; }
.parameter-meta { display: flex; flex-wrap: wrap; gap: 10px; color: var(--text-faint); font-size: 11px; }
.object-list { border-top: 1px solid var(--border-soft); background: var(--bg-subtle); }
.object-row { display: grid; grid-template-columns: minmax(180px, 1fr) 120px minmax(140px, 0.8fr) minmax(180px, 1fr); gap: 10px; align-items: center; padding: 7px 10px; color: var(--text-muted); font-size: 11px; }
.object-row + .object-row { border-top: 1px solid var(--border-soft); }
.object-row > div { display: flex; flex-direction: column; min-width: 0; }
.object-row strong, .object-row code { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
@media (max-width: 900px) {
  .parameter-result__main, .object-row { grid-template-columns: 1fr; }
  .query-row { align-items: flex-start; flex-direction: column; }
}
@media (prefers-reduced-motion: reduce) {
  * { transition: none !important; }
}
</style>
