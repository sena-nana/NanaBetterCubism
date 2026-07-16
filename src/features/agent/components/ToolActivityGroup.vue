<script setup lang="ts">
import ChevronRight from "@lucide/vue/dist/esm/icons/chevron-right.mjs";
import { computed, ref } from "vue";
import { toolActivityPresentation } from "../conversationPresentation";
import { toolActivityGroupPresentation } from "../toolActivityGroups";
import type { ChatMessage } from "../types";

const props = withDefaults(
  defineProps<{
    messages: ChatMessage[];
    agentIdPrefix?: string;
  }>(),
  { agentIdPrefix: "agent.chat" },
);

const expanded = ref(false);
const groupId = computed(() => props.messages[0]?.id ?? "empty");
const panelId = computed(() => `${props.agentIdPrefix}-tool-group-${groupId.value}`);
const summary = computed(() => toolActivityGroupPresentation(props.messages));
const items = computed(() => props.messages.map((message) => ({
  message,
  presentation: toolActivityPresentation(message),
})));
const statusLabels = {
  started: "进行中",
  finished: "完成",
  failed: "失败",
  unknown: null,
} as const;
</script>

<template>
  <section
    v-if="messages.length"
    class="tool-activity-group"
    :class="`is-${summary.status}`"
    :data-agent-id="`${agentIdPrefix}.tool-group.${groupId}`"
    :data-status="summary.status"
    :data-summary-mode="summary.mode"
    :data-operation-count="summary.count"
  >
    <button
      type="button"
      class="tool-activity-group__toggle"
      :aria-controls="panelId"
      :aria-expanded="expanded"
      :data-agent-id="`${agentIdPrefix}.tool-group.${groupId}.toggle`"
      @click="expanded = !expanded"
    >
      <span class="tool-activity-group__status" aria-hidden="true" />
      <span class="tool-activity-group__summary" role="status" aria-live="polite" aria-atomic="true">
        <span class="tool-activity-group__label">{{ summary.label }}</span>
        <span v-if="statusLabels[summary.status]" class="tool-activity-group__meta">
          {{ statusLabels[summary.status] }}
        </span>
      </span>
      <ChevronRight
        class="tool-activity-group__chevron"
        :class="{ 'is-expanded': expanded }"
        :size="14"
        aria-hidden="true"
      />
    </button>

    <ol v-if="expanded" :id="panelId" class="tool-activity-group__list">
      <li
        v-for="{ message, presentation } in items"
        :key="message.id"
        class="tool-activity-group__item"
        :class="`is-${presentation.status}`"
        :data-agent-id="`${agentIdPrefix}.tool.${message.id}`"
        :data-status="presentation.status"
      >
        <div class="tool-activity-group__item-row">
          <span class="tool-activity-group__item-status" aria-hidden="true" />
          <span class="tool-activity-group__item-label">{{ presentation.label }}</span>
          <span v-if="statusLabels[presentation.status]" class="tool-activity-group__item-meta">
            {{ statusLabels[presentation.status] }}
          </span>
        </div>
        <p v-if="presentation.detail" class="tool-activity-group__error" role="alert">
          {{ presentation.detail }}
        </p>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.tool-activity-group {
  width: fit-content;
  max-width: 100%;
  min-width: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.tool-activity-group__toggle,
.tool-activity-group__item-row {
  display: flex;
  align-items: center;
}

.tool-activity-group__toggle {
  width: fit-content;
  max-width: 100%;
  min-height: 26px;
  gap: 7px;
  padding: 3px 7px 3px 8px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: inherit;
  cursor: pointer;
  font: inherit;
  text-align: left;
  transition: border-color 0.12s ease, background 0.12s ease, color 0.12s ease;
}

.tool-activity-group__toggle:hover {
  border-color: var(--border);
  background: var(--bg-hover);
  color: var(--text);
}

.tool-activity-group__toggle:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.tool-activity-group__status,
.tool-activity-group__item-status {
  width: 7px;
  height: 7px;
  flex: 0 0 auto;
  border-radius: 50%;
  background: var(--text-faint);
}

.tool-activity-group.is-started .tool-activity-group__status,
.tool-activity-group__item.is-started .tool-activity-group__item-status {
  background: var(--warn);
  animation: tool-activity-pulse 1.2s ease-in-out infinite;
}

.tool-activity-group.is-finished .tool-activity-group__status,
.tool-activity-group__item.is-finished .tool-activity-group__item-status {
  background: var(--ok);
}

.tool-activity-group.is-failed .tool-activity-group__status,
.tool-activity-group__item.is-failed .tool-activity-group__item-status {
  background: var(--err);
}

.tool-activity-group__summary {
  display: flex;
  min-width: 0;
  align-items: baseline;
  gap: 7px;
}

.tool-activity-group__label,
.tool-activity-group__item-label {
  min-width: 0;
  color: var(--text);
}

.tool-activity-group__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-activity-group__meta,
.tool-activity-group__item-meta {
  flex: 0 0 auto;
  color: var(--text-faint);
  font-size: 11px;
}

.tool-activity-group__chevron {
  flex: 0 0 auto;
  color: var(--text-faint);
  transition: transform 0.12s ease;
}

.tool-activity-group__chevron.is-expanded {
  transform: rotate(90deg);
}

.tool-activity-group__list {
  display: flex;
  flex-direction: column;
  gap: 3px;
  margin: 5px 0 0 10px;
  padding: 2px 0 1px 10px;
  border-left: 1px solid var(--border-soft);
  list-style: none;
}

.tool-activity-group__item {
  min-width: 0;
  padding: 2px 4px;
  border-radius: var(--radius-xs);
}

.tool-activity-group__item-row {
  min-height: 22px;
  gap: 7px;
}

.tool-activity-group__item-label {
  flex: 1 1 auto;
  overflow-wrap: anywhere;
}

.tool-activity-group__error {
  margin: 1px 0 3px 14px;
  color: var(--err);
  font-size: 12px;
  line-height: 1.5;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

@keyframes tool-activity-pulse {
  50% { opacity: 0.38; }
}

@media (prefers-reduced-motion: reduce) {
  .tool-activity-group.is-started .tool-activity-group__status,
  .tool-activity-group__item.is-started .tool-activity-group__item-status {
    animation: none;
  }

  .tool-activity-group__chevron,
  .tool-activity-group__toggle {
    transition: none;
  }
}
</style>
