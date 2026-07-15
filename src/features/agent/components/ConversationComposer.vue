<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { UiButton, UiTextarea } from "@lilia/ui";
import type { PendingAsk } from "../types";

type TextareaRef = { $el?: HTMLTextAreaElement } | HTMLTextAreaElement | null;

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

const inputRef = ref<TextareaRef>(null);
const askRef = ref<TextareaRef>(null);

function textareaElement(value: TextareaRef) {
  if (value instanceof HTMLTextAreaElement) return value;
  return value?.$el instanceof HTMLTextAreaElement ? value.$el : null;
}

function resize(value: TextareaRef) {
  const textarea = textareaElement(value);
  if (!textarea) return;
  textarea.style.height = "auto";
  textarea.style.height = `${Math.min(180, Math.max(48, textarea.scrollHeight))}px`;
}

watch(
  () => [props.modelValue, props.askAnswer, props.pendingAsk?.askId],
  () => void nextTick(() => {
    resize(inputRef.value);
    resize(askRef.value);
  }),
  { immediate: true },
);

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
          v-for="(option, index) in pendingAsk.options"
          :key="option"
          size="sm"
          :agent-id="`${agentIdPrefix}.ask-option.${index}`"
          @click="emit('answer', option)"
        >
          {{ option }}
        </UiButton>
      </div>
      <div class="conversation-composer__answer">
        <UiTextarea
          ref="askRef"
          :model-value="askAnswer"
          rows="1"
          placeholder="输入回答"
          :agent-id="`${agentIdPrefix}.ask-input`"
          @update:model-value="emit('update:askAnswer', $event)"
          @keydown="onAskKeydown"
        />
        <div class="conversation-composer__answer-actions">
          <UiButton size="sm" :agent-id="`${agentIdPrefix}.cancel`" @click="emit('cancel')">取消</UiButton>
          <UiButton variant="primary" size="sm" :disabled="!askAnswer.trim()" :agent-id="`${agentIdPrefix}.ask-submit`" @click="emit('answer')">回答</UiButton>
        </div>
      </div>
    </div>

    <template v-else>
      <UiTextarea
        ref="inputRef"
        :model-value="modelValue"
        rows="1"
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
          :agent-id="`${agentIdPrefix}.stop`"
          @click="emit('cancel')"
        >
          {{ cancelling ? "正在停止" : "停止" }}
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
  min-width: 0;
  width: 100%;
  padding: 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-elev);
  box-shadow: 0 4px 16px -8px rgba(0, 0, 0, 0.45);
}

.conversation-composer__toolbar,
.conversation-composer__actions,
.conversation-composer__answer-actions,
.conversation-composer__options { display: flex; align-items: center; gap: 6px; }
.conversation-composer__toolbar { min-height: 28px; padding: 0 1px 6px; overflow-x: auto; }
.conversation-composer :deep(.ui-textarea) { display: block; width: 100%; min-height: 48px; max-height: 180px; resize: none; overflow-y: auto; border: 0; background: transparent; box-shadow: none; line-height: 1.55; }
.conversation-composer :deep(.ui-textarea:focus) { outline: none; }
.conversation-composer__actions { justify-content: flex-end; min-height: 30px; padding: 6px 1px 0; }
.conversation-composer__hint { margin-right: auto; color: var(--text-faint); font-size: 11px; }
.conversation-composer__pending { min-height: 108px; padding: 3px; }
.conversation-composer__question { margin: 0 0 9px; font-size: 13px; line-height: 1.5; }
.conversation-composer__options { flex-wrap: wrap; margin-bottom: 7px; }
.conversation-composer__answer { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: end; gap: 7px; }
.conversation-composer__answer-actions { padding-bottom: 1px; }
.conversation-composer__error { margin: 6px 3px 0; color: var(--err); font-size: 12px; }

@media (max-width: 620px) {
  .conversation-composer__hint { display: none; }
  .conversation-composer__answer { grid-template-columns: 1fr; }
  .conversation-composer__answer-actions { justify-content: flex-end; }
}
</style>
