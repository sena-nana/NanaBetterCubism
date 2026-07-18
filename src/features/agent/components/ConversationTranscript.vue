<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { Button } from "../../../ui";
import { buildConversationTimeline } from "../toolActivityGroups";
import MarkdownBlock from "../markdown/MarkdownBlock.vue";
import ToolActivityGroup from "./ToolActivityGroup.vue";
import type { ChatMessage } from "../types";

const props = withDefaults(
  defineProps<{
    messages: ChatMessage[];
    loading?: boolean;
    running?: boolean;
    cancelling?: boolean;
    emptyTitle?: string;
    emptyDescription?: string;
    agentIdPrefix?: string;
  }>(),
  {
    loading: false,
    running: false,
    cancelling: false,
    emptyTitle: "想在 Cubism Editor 中完成什么？",
    emptyDescription: "描述目标后，Agent 会基于当前连接和可用能力继续。",
    agentIdPrefix: "agent.chat",
  },
);

const scroller = ref<HTMLElement | null>(null);
const followOutput = ref(true);
const timeline = computed(() => buildConversationTimeline(props.messages));

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
  followOutput.value = true;
}

watch(
  () => props.messages.map((message) => `${message.id}:${message.content.length}:${message.toolStatus}`).join("|"),
  () => void scrollToBottom(),
);

watch(
  () => [props.running, props.cancelling],
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
  <div class="conversation-transcript-frame">
    <div
      ref="scroller"
      class="conversation-transcript"
      :data-agent-id="`${agentIdPrefix}.transcript`"
      @scroll.passive="updateFollowState"
    >
      <div v-if="loading" class="conversation-transcript__state" :data-agent-id="`${agentIdPrefix}.loading`">
        <span>正在打开对话…</span>
      </div>

      <div
        v-else-if="messages.length === 0 && !running && !cancelling"
        class="conversation-transcript__state conversation-transcript__state--empty"
        :data-agent-id="`${agentIdPrefix}.empty`"
      >
        <h1>{{ emptyTitle }}</h1>
        <p>{{ emptyDescription }}</p>
      </div>

      <div v-else class="conversation-transcript__timeline">
        <article
          v-for="entry in timeline"
          :key="entry.key"
          class="timeline-entry"
          :class="entry.kind === 'tool-group' ? 'timeline-entry--tool' : `timeline-entry--${entry.message.role}`"
          :data-agent-id="entry.kind === 'message' ? `${agentIdPrefix}.message.${entry.message.id}` : undefined"
          :data-agent-selectable="entry.kind === 'message' && entry.message.role === 'assistant' ? 'true' : undefined"
        >
          <ToolActivityGroup
            v-if="entry.kind === 'tool-group'"
            :messages="entry.messages"
            :agent-id-prefix="agentIdPrefix"
          />
          <p v-else-if="entry.message.role === 'user'" class="timeline-entry__user-body">
            {{ entry.message.content }}
          </p>
          <MarkdownBlock v-else-if="entry.message.role === 'assistant'" :content="entry.message.content" />
          <p v-else class="timeline-entry__system-body">{{ entry.message.content }}</p>
        </article>

        <div
          v-if="running || cancelling"
          class="conversation-progress"
          :data-agent-id="`${agentIdPrefix}.progress`"
          role="status"
          aria-live="polite"
        >
          <span class="conversation-progress__dot" aria-hidden="true" />
          <span>{{ cancelling ? "正在停止" : "正在处理" }}</span>
        </div>
      </div>
    </div>

    <Button
      v-if="!followOutput && messages.length"
      class="conversation-transcript__latest"
      size="sm"
      :agent-id="`${agentIdPrefix}.scroll-latest`"
      @click="scrollToBottom(true)"
    >
      回到最新
    </Button>
  </div>
</template>

<style scoped>
.conversation-transcript-frame {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 0;
}

.conversation-transcript {
  height: 100%;
  min-height: 0;
  overflow: auto;
  padding: 24px clamp(16px, 7vw, 96px) 24px;
  scrollbar-gutter: stable;
}

.conversation-transcript__timeline {
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: min(860px, 100%);
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

.conversation-transcript__state--empty { flex-direction: column; gap: 8px; }
.conversation-transcript__state h1 { margin: 0; color: var(--text); font-size: clamp(20px, 2.6vw, 26px); font-weight: 600; }
.conversation-transcript__state p { max-width: 520px; margin: 0; line-height: 1.6; }

.timeline-entry { min-width: 0; color: var(--text); }
.timeline-entry--user { align-self: flex-end; max-width: min(680px, 78%); padding: 9px 12px; border-radius: 12px; background: var(--bg-active); }
.timeline-entry--assistant { align-self: stretch; padding-inline: 2px; }
.timeline-entry--tool, .timeline-entry--system { align-self: flex-start; max-width: min(680px, 92%); }
.timeline-entry__user-body, .timeline-entry__system-body { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 13px; line-height: 1.6; }
.timeline-entry__system-body { padding: 7px 9px; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); background: var(--bg-subtle); color: var(--text-muted); font-size: 12px; }

.conversation-progress { display: inline-flex; align-items: center; gap: 7px; align-self: flex-start; min-height: 28px; color: var(--text-muted); font-size: 12px; }
.conversation-progress__dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); animation: activity-pulse 1.2s ease-in-out infinite; }
.conversation-transcript__latest { position: absolute; right: 18px; bottom: 12px; z-index: 2; box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18); }

@keyframes activity-pulse { 50% { opacity: 0.38; } }

@media (prefers-reduced-motion: reduce) {
  .conversation-progress__dot { animation: none; }
}

@media (max-width: 720px) {
  .conversation-transcript { padding: 18px 14px 20px; }
  .timeline-entry--user { max-width: 88%; }
  .conversation-transcript__latest { right: 12px; }
}
</style>
