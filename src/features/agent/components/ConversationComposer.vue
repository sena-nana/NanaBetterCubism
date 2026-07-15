<script setup lang="ts">
import { UiButton, UiTextarea } from "@lilia/ui";
import type { PendingAsk } from "../types";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    askAnswer?: string;
    pendingAsk?: PendingAsk | null;
    disabled?: boolean;
    running?: boolean;
    cancelling?: boolean;
    canSend?: boolean;
    error?: string | null;
    placeholder?: string;
    agentIdPrefix?: string;
  }>(),
  {
    askAnswer: "",
    pendingAsk: null,
    disabled: false,
    running: false,
    cancelling: false,
    canSend: false,
    error: null,
    placeholder: "描述你想在 Cubism Editor 中完成的事…",
    agentIdPrefix: "agent.chat",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  "update:askAnswer": [value: string];
  send: [];
  cancel: [];
  answer: [answer?: string];
}>();

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  if (props.canSend) emit("send");
}

function onAskKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  if (props.askAnswer.trim()) emit("answer");
}
</script>

<template>
  <section class="conversation-composer" :data-agent-id="`${agentIdPrefix}.composer`">
    <div v-if="$slots.toolbar" class="conversation-composer__toolbar">
      <slot name="toolbar" />
    </div>

    <div v-if="pendingAsk" class="conversation-composer__pending" :data-agent-id="`${agentIdPrefix}.ask`">
      <p class="conversation-composer__question">{{ pendingAsk.question }}</p>
      <div v-if="pendingAsk.options.length" class="conversation-composer__options">
        <UiButton
          v-for="option in pendingAsk.options"
          :key="option"
          size="sm"
          :agent-id="`${agentIdPrefix}.ask-option.${option}`"
          @click="emit('answer', option)"
        >
          {{ option }}
        </UiButton>
      </div>
      <div class="conversation-composer__answer">
        <UiTextarea
          :model-value="askAnswer"
          rows="2"
          placeholder="输入回答"
          :agent-id="`${agentIdPrefix}.ask-input`"
          @update:model-value="emit('update:askAnswer', $event)"
          @keydown="onAskKeydown"
        />
        <UiButton
          variant="primary"
          size="sm"
          :disabled="!askAnswer.trim()"
          :agent-id="`${agentIdPrefix}.ask-submit`"
          @click="emit('answer')"
        >
          回答
        </UiButton>
        <UiButton size="sm" :agent-id="`${agentIdPrefix}.cancel`" @click="emit('cancel')">
          取消
        </UiButton>
      </div>
    </div>

    <template v-else>
      <UiTextarea
        :model-value="modelValue"
        rows="3"
        :placeholder="placeholder"
        :disabled="disabled || running || cancelling"
        :agent-id="`${agentIdPrefix}.input`"
        @update:model-value="emit('update:modelValue', $event)"
        @keydown="onKeydown"
      />
      <div class="conversation-composer__actions">
        <span class="conversation-composer__hint">Enter 发送 · Shift+Enter 换行</span>
        <UiButton
          v-if="running || cancelling"
          size="sm"
          :busy="cancelling"
          :agent-id="`${agentIdPrefix}.cancel`"
          @click="emit('cancel')"
        >
          {{ cancelling ? "正在取消" : "停止" }}
        </UiButton>
        <UiButton
          v-else
          variant="primary"
          size="sm"
          :disabled="!canSend"
          :agent-id="`${agentIdPrefix}.send`"
          @click="emit('send')"
        >
          发送
        </UiButton>
      </div>
    </template>

    <p v-if="error" class="conversation-composer__error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.conversation-composer {
  width: min(900px, 100%);
  margin: 0 auto;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-elev);
}

.conversation-composer__toolbar,
.conversation-composer__actions,
.conversation-composer__answer,
.conversation-composer__options {
  display: flex;
  align-items: center;
  gap: 6px;
}

.conversation-composer__toolbar {
  min-height: 28px;
  padding: 0 2px 6px;
  overflow-x: auto;
}

.conversation-composer :deep(.ui-textarea) {
  display: block;
  width: 100%;
  min-height: 68px;
  max-height: 180px;
  resize: vertical;
  border: 0;
  background: transparent;
  box-shadow: none;
}

.conversation-composer :deep(.ui-textarea:focus) {
  outline: none;
}

.conversation-composer__actions {
  justify-content: flex-end;
  min-height: 30px;
  padding: 6px 2px 0;
}

.conversation-composer__hint {
  margin-right: auto;
  color: var(--text-faint);
  font-size: 11px;
}

.conversation-composer__pending {
  min-height: 118px;
  padding: 4px;
}

.conversation-composer__question {
  margin: 0 0 10px;
  font-size: 13px;
  line-height: 1.5;
}

.conversation-composer__options {
  flex-wrap: wrap;
  margin-bottom: 8px;
}

.conversation-composer__answer {
  align-items: flex-end;
}

.conversation-composer__answer :deep(.ui-textarea) {
  min-height: 54px;
}

.conversation-composer__error {
  margin: 6px 4px 0;
  color: var(--err);
  font-size: 12px;
}
</style>
