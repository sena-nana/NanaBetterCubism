<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from "vue";
import type { ChatMessage } from "../types";

const props = withDefaults(
  defineProps<{
    messages: ChatMessage[];
    loading?: boolean;
    emptyTitle?: string;
    emptyDescription?: string;
  }>(),
  {
    loading: false,
    emptyTitle: "想在 Cubism Editor 中完成什么？",
    emptyDescription: "描述目标后，Agent 会基于当前连接和可用能力继续。",
  },
);

const scroller = ref<HTMLElement | null>(null);
const followOutput = ref(true);

function updateFollowState() {
  const element = scroller.value;
  if (!element) return;
  followOutput.value = element.scrollHeight - element.scrollTop - element.clientHeight < 96;
}

async function scrollToBottom(force = false) {
  await nextTick();
  const element = scroller.value;
  if (!element || (!force && !followOutput.value)) return;
  element.scrollTop = element.scrollHeight;
}

watch(
  () => props.messages.map((message) => `${message.id}:${message.content.length}:${message.toolStatus}`).join("|"),
  () => void scrollToBottom(),
);

watch(
  () => props.loading,
  (loading, previous) => {
    if (previous && !loading) void scrollToBottom(true);
  },
);

onMounted(() => void scrollToBottom(true));
</script>

<template>
  <div
    ref="scroller"
    class="conversation-transcript"
    data-agent-id="agent.chat.transcript"
    @scroll.passive="updateFollowState"
  >
    <div v-if="loading" class="conversation-transcript__state" data-agent-id="agent.chat.loading">
      <span>正在打开对话…</span>
    </div>

    <div
      v-else-if="messages.length === 0"
      class="conversation-transcript__state conversation-transcript__state--empty"
      data-agent-id="agent.chat.empty"
    >
      <h1>{{ emptyTitle }}</h1>
      <p>{{ emptyDescription }}</p>
    </div>

    <div v-else class="conversation-transcript__timeline">
      <article
        v-for="message in messages"
        :key="message.id"
        class="timeline-entry"
        :class="`timeline-entry--${message.role}`"
        :data-agent-id="`agent.chat.message.${message.id}`"
      >
        <div v-if="message.role === 'tool'" class="timeline-entry__tool-meta">
          <span
            class="timeline-entry__status"
            :class="`is-${message.toolStatus ?? 'unknown'}`"
            aria-hidden="true"
          />
          <span>{{ message.toolName || "工具" }}</span>
        </div>
        <p class="timeline-entry__body">{{ message.content }}</p>
      </article>
    </div>
  </div>
</template>

<style scoped>
.conversation-transcript {
  height: 100%;
  min-height: 0;
  overflow: auto;
  padding: 20px clamp(16px, 7vw, 96px) 12px;
  scrollbar-gutter: stable;
}

.conversation-transcript__timeline {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: min(880px, 100%);
  min-height: 100%;
  margin: 0 auto;
}

.conversation-transcript__state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 100%;
  color: var(--text-muted);
  font-size: 13px;
  text-align: center;
}

.conversation-transcript__state--empty {
  flex-direction: column;
  gap: 8px;
}

.conversation-transcript__state h1 {
  margin: 0;
  color: var(--text);
  font-size: clamp(22px, 3vw, 30px);
  font-weight: 600;
}

.conversation-transcript__state p {
  max-width: 520px;
  margin: 0;
  line-height: 1.6;
}

.timeline-entry {
  width: fit-content;
  max-width: min(760px, 92%);
  color: var(--text);
}

.timeline-entry--user {
  align-self: flex-end;
  padding: 9px 12px;
  border-radius: var(--radius-md);
  background: var(--bg-active);
}

.timeline-entry--assistant {
  align-self: flex-start;
  padding-inline: 2px;
}

.timeline-entry--tool,
.timeline-entry--system {
  align-self: flex-start;
  width: min(680px, 92%);
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 12px;
}

.timeline-entry__tool-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
  color: var(--text-faint);
  font-size: 11px;
}

.timeline-entry__status {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-faint);
}

.timeline-entry__status.is-started {
  background: var(--warn);
}

.timeline-entry__status.is-finished {
  background: var(--ok);
}

.timeline-entry__status.is-failed {
  background: var(--err);
}

.timeline-entry__body {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 13px;
  line-height: 1.65;
}

@media (max-width: 720px) {
  .conversation-transcript {
    padding-inline: 14px;
  }
}
</style>
