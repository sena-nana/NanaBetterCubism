<script setup lang="ts">
import RefreshCw from "@lucide/vue/dist/esm/icons/refresh-cw.mjs";
import { Button, EmptyState, IconButton, Input } from "../../../ui";
import { computed } from "vue";
import { formatMemoryTime } from "../memoryPresentation";
import type { MemoryRecord, MemoryScope, ProjectRecord } from "../types";

const props = defineProps<{
  scope: MemoryScope;
  projects: ProjectRecord[];
  projectId: string;
  search: string;
  memories: MemoryRecord[];
  selectedId: string | null;
  loading: boolean;
  refreshing: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  "update:projectId": [value: string];
  "update:search": [value: string];
  select: [id: string];
  refresh: [];
  retry: [];
}>();

const emptyTitle = computed(() => (props.search.trim() ? "没有匹配的记忆" : "暂无记忆"));
const emptyMessage = computed(() =>
  props.search.trim()
    ? "尝试缩短关键词或清除搜索。"
    : props.scope === "project"
      ? "Agent 保存的项目进展会显示在这里。"
      : "Agent 保存的跨项目经验会显示在这里。",
);

function updateProject(event: Event) {
  emit("update:projectId", (event.target as HTMLSelectElement).value);
}
</script>

<template>
  <aside class="memory-list-pane" data-agent-id="agent.memory.list">
    <div class="memory-list-pane__tools">
      <label v-if="scope === 'project'" class="memory-filter">
        <span>项目</span>
        <select
          :value="projectId"
          :disabled="loading"
          data-agent-id="agent.memory.project-filter"
          @change="updateProject"
        >
          <option value="">全部项目</option>
          <option v-for="project in projects" :key="project.id" :value="project.id">
            {{ project.name }}
          </option>
        </select>
      </label>

      <div class="memory-search">
        <Input
          :model-value="search"
          type="search"
          aria-label="搜索记忆"
          placeholder="搜索标题、项目或正文"
          agent-id="agent.memory.search"
          @update:model-value="emit('update:search', $event)"
        />
        <IconButton
          :icon="RefreshCw"
          label="刷新记忆"
          size="md"
          :loading="refreshing"
          :disabled="loading"
          agent-id="agent.memory.refresh"
          @click="emit('refresh')"
        />
      </div>
    </div>

    <div class="memory-list-pane__body">
      <EmptyState
        v-if="loading"
        title="正在加载记忆"
        message="正在读取本地记忆。"
        agent-id="agent.memory.loading"
      />

      <div v-else-if="error" class="memory-list-error" role="alert" data-agent-id="agent.memory.error">
        <p>{{ error }}</p>
        <Button size="sm" agent-id="agent.memory.retry" @click="emit('retry')">重试</Button>
      </div>

      <EmptyState
        v-else-if="!memories.length"
        :title="emptyTitle"
        :message="emptyMessage"
        agent-id="agent.memory.empty"
      />

      <div v-else class="memory-list" role="listbox" aria-label="记忆列表">
        <button
          v-for="memory in memories"
          :key="memory.id"
          type="button"
          class="memory-row"
          :class="{
            'is-selected': memory.id === selectedId,
            'is-disabled': !memory.enabled,
          }"
          role="option"
          :aria-selected="memory.id === selectedId"
          :data-agent-id="`agent.memory.row.${memory.id}`"
          @click="emit('select', memory.id)"
        >
          <span class="memory-row__heading">
            <strong>{{ memory.title }}</strong>
            <span class="memory-row__status" :class="memory.enabled ? 'is-enabled' : 'is-disabled'">
              {{ memory.enabled ? "启用" : "停用" }}
            </span>
          </span>
          <span v-if="scope === 'project'" class="memory-row__project">
            {{ memory.projectName ?? "未命名项目" }}
          </span>
          <span class="memory-row__overview">
            {{ memory.layers[0]?.content || "摘要层暂无内容" }}
          </span>
          <span class="memory-row__time">{{ formatMemoryTime(memory.updatedAt) }}</span>
        </button>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.memory-list-pane {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  min-width: 0;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-elev);
  overflow: hidden;
}

.memory-list-pane__tools {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-bottom: 1px solid var(--border-soft);
}

.memory-filter {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  color: var(--text-muted);
  font-size: 12px;
}

.memory-filter select {
  min-width: 0;
  height: 30px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg);
  color: var(--text);
}

.memory-search {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 6px;
}

.memory-search :deep(.ui-input) {
  width: 100%;
}

.memory-list-pane__body {
  min-height: 0;
  overflow: auto;
}

.memory-list-pane__body :deep(.ui-empty-state) {
  min-height: 160px;
  justify-content: center;
}

.memory-list-error {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
  padding: 18px;
}

.memory-list-error p {
  margin: 0;
  color: var(--err);
  font-size: 12px;
  line-height: 1.5;
}

.memory-list {
  display: flex;
  flex-direction: column;
  padding: 5px;
}

.memory-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  padding: 9px 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease;
}

.memory-row:hover {
  background: var(--bg-hover);
}

.memory-row.is-selected {
  background: var(--bg-active);
}

.memory-row.is-disabled {
  color: var(--text-muted);
}

.memory-row__heading {
  display: flex;
  align-items: center;
  gap: 8px;
}

.memory-row__heading strong {
  min-width: 0;
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 600;
}

.memory-row__status {
  flex: 0 0 auto;
  font-size: 10px;
}

.memory-row__status.is-enabled {
  color: var(--ok);
}

.memory-row__status.is-disabled {
  color: var(--text-faint);
}

.memory-row__project,
.memory-row__time {
  color: var(--text-faint);
  font-size: 11px;
}

.memory-row__overview {
  display: -webkit-box;
  overflow: hidden;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.45;
  overflow-wrap: anywhere;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
}

@media (prefers-reduced-motion: reduce) {
  .memory-row {
    transition: none;
  }
}
</style>
