<script setup lang="ts">
import { UiEmptyState, UiSwitch } from "@lilia/ui";
import { formatMemoryTime, memoryLayerLabel } from "../memoryPresentation";
import MarkdownBlock from "../markdown/MarkdownBlock.vue";
import type { MemoryRecord } from "../types";

defineProps<{
  memory: MemoryRecord | null;
  busy: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  toggle: [memory: MemoryRecord, enabled: boolean];
}>();
</script>

<template>
  <main class="memory-detail-pane" data-agent-id="agent.memory.detail">
    <UiEmptyState
      v-if="!memory"
      title="选择一条记忆"
      message="从左侧列表选择记忆后可查看各层内容。"
      agent-id="agent.memory.detail.empty"
    />

    <template v-else>
      <header class="memory-detail-header">
        <div class="memory-detail-header__identity">
          <div class="memory-detail-header__eyebrow">
            <span>{{ memory.scope === "project" ? "项目记忆" : "全局记忆" }}</span>
            <span v-if="memory.projectName">{{ memory.projectName }}</span>
          </div>
          <h2>{{ memory.title }}</h2>
          <div class="memory-detail-header__meta">
            <span>更新于 {{ formatMemoryTime(memory.updatedAt) }}</span>
            <RouterLink
              v-if="memory.sourceConversationId"
              :to="`/chats/${memory.sourceConversationId}`"
              :data-agent-id="`agent.memory.source.${memory.id}`"
            >
              查看来源对话
            </RouterLink>
          </div>
        </div>

        <UiSwitch
          :model-value="memory.enabled"
          :label="memory.enabled ? '已启用' : '已停用'"
          aria-label="启用记忆"
          control-position="end"
          :disabled="busy"
          :agent-id="`agent.memory.enable.${memory.id}`"
          @update:model-value="emit('toggle', memory, Boolean($event))"
        />
      </header>

      <p v-if="error" class="memory-detail-error" role="alert" data-agent-id="agent.memory.action-error">
        {{ error }}
      </p>

      <div class="memory-layers">
        <section
          v-for="layer in memory.layers"
          :key="layer.name"
          class="memory-layer"
          :data-layer="layer.name"
          :data-agent-id="`agent.memory.layer.${memory.id}.${layer.name}`"
        >
          <header class="memory-layer__header">
            <h3>{{ memoryLayerLabel(layer.name) }}</h3>
            <span>{{ layer.name }}</span>
          </header>
          <MarkdownBlock v-if="layer.content" :content="layer.content" />
          <p v-else class="memory-layer__empty">此层暂无内容。</p>
        </section>
      </div>
    </template>
  </main>
</template>

<style scoped>
.memory-detail-pane {
  min-width: 0;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-elev);
}

.memory-detail-pane > :deep(.ui-empty-state) {
  min-height: 240px;
  justify-content: center;
}

.memory-detail-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--border-soft);
}

.memory-detail-header__identity {
  min-width: 0;
}

.memory-detail-header__eyebrow,
.memory-detail-header__meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 10px;
  color: var(--text-muted);
  font-size: 11px;
}

.memory-detail-header__eyebrow span + span::before {
  content: "·";
  margin-right: 10px;
  color: var(--text-faint);
}

.memory-detail-header h2 {
  margin: 5px 0 7px;
  color: var(--text);
  font-size: 17px;
  font-weight: 600;
  line-height: 1.35;
  overflow-wrap: anywhere;
}

.memory-detail-header__meta a {
  color: var(--accent);
  text-decoration: none;
}

.memory-detail-header__meta a:hover {
  text-decoration: underline;
}

.memory-detail-error {
  margin: 12px 18px 0;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--err-soft);
  color: var(--err);
  font-size: 12px;
}

.memory-layers {
  display: flex;
  flex-direction: column;
  padding: 4px 18px 18px;
}

.memory-layer {
  padding: 14px 0;
  border-bottom: 1px solid var(--border-soft);
}

.memory-layer:last-child {
  border-bottom: 0;
}

.memory-layer__header {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 8px;
}

.memory-layer__header h3 {
  margin: 0;
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
}

.memory-layer__header span {
  color: var(--text-faint);
  font-size: 10px;
}

.memory-layer :deep(.markdown-block) {
  font-size: 13px;
}

.memory-layer__empty {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

@media (max-width: 720px) {
  .memory-detail-header {
    flex-direction: column;
  }
}
</style>
