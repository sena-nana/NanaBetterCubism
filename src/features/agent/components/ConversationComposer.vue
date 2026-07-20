<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { Button, Textarea } from "../../../ui";
import { ImagePlus, ListChecks, X } from "@lucide/vue";
import { chatImageSrc, MAX_CHAT_IMAGES } from "../useChatImageDrafts";
import type {
  AgentTurnMode,
  ChatImageDraft,
  ComputerActionKind,
  ComputerOperationStatus,
  PendingUserAction,
} from "../types";

type TextareaRef = { $el?: HTMLTextAreaElement } | HTMLTextAreaElement | null;

const props = withDefaults(
  defineProps<{
    modelValue: string;
    askAnswer?: string;
    planRevision?: string;
    mode?: AgentTurnMode;
    pendingAction?: PendingUserAction | null;
    computerStatus?: ComputerOperationStatus;
    disabled?: boolean;
    running?: boolean;
    cancelling?: boolean;
    canSend?: boolean;
    images?: ChatImageDraft[];
    error?: string | null;
    placeholder?: string;
    agentIdPrefix?: string;
    imageInputDisabled?: boolean;
  }>(),
  {
    askAnswer: "",
    planRevision: "",
    mode: "default",
    pendingAction: null,
    computerStatus: "idle",
    disabled: false,
    running: false,
    cancelling: false,
    canSend: false,
    images: () => [],
    error: null,
    placeholder: "描述你想在 Cubism Editor 中完成的事…",
    agentIdPrefix: "agent.chat",
    imageInputDisabled: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  "update:askAnswer": [value: string];
  "update:planRevision": [value: string];
  "update:mode": [value: AgentTurnMode];
  send: [];
  cancel: [];
  answer: [answer?: string];
  decide: [approved: boolean];
  decidePlan: [decision: "approve" | "revise" | "cancel", revision?: string];
  pickImages: [];
  removeImage: [draftId: string];
  viewImage: [image: ChatImageDraft];
  paste: [event: ClipboardEvent];
}>();

const inputRef = ref<TextareaRef>(null);
const askRef = ref<TextareaRef>(null);
const planRevisionRef = ref<TextareaRef>(null);

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

const pendingQuestion = computed(() =>
  props.pendingAction?.kind === "question" ? props.pendingAction : null,
);
const computerApproval = computed(() =>
  props.pendingAction?.kind === "computer_approval" ? props.pendingAction : null,
);
const planApproval = computed(() =>
  props.pendingAction?.kind === "plan_approval" ? props.pendingAction : null,
);

const actionLabels: Record<ComputerActionKind, string> = {
  click: "点击",
  double_click: "双击",
  drag: "拖动",
  scroll: "滚动",
  key: "快捷键",
  type_text: "文本输入",
};

const statusLabels: Record<ComputerOperationStatus, string> = {
  idle: "",
  awaiting_approval: "等待授权",
  authorized: "已授权",
  running: "正在操作 Cubism",
  completed: "操作完成",
  needs_user_verification: "需要用户核对",
  cancelled: "已取消",
  failed: "操作失败",
  unknown: "结果未知",
};

function expiryLabel(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "授权即将过期"
    : `授权于 ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })} 过期`;
}

watch(
  () => [props.modelValue, props.askAnswer, props.planRevision, props.pendingAction?.actionId],
  () => void nextTick(() => {
    resize(inputRef.value);
    resize(askRef.value);
    resize(planRevisionRef.value);
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

function onPlanRevisionKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  const revision = props.planRevision.trim();
  if (revision) emit("decidePlan", "revise", revision);
}
</script>

<template>
  <section class="conversation-composer" :data-agent-id="`${agentIdPrefix}.composer`">
    <div v-if="$slots.toolbar" class="conversation-composer__toolbar">
      <slot name="toolbar" />
    </div>

    <div
      v-if="planApproval"
      class="conversation-composer__pending conversation-composer__plan-approval"
      :data-agent-id="`${agentIdPrefix}.plan-approval`"
    >
      <div class="conversation-composer__approval-heading">
        <strong>计划确认</strong>
        <span>{{ planApproval.title }}</span>
      </div>
      <Textarea
        ref="planRevisionRef"
        :model-value="planRevision"
        :rows="1"
        placeholder="输入修改要求"
        :agent-id="`${agentIdPrefix}.plan-approval.revision`"
        @update:model-value="emit('update:planRevision', $event)"
        @keydown="onPlanRevisionKeydown"
      />
      <div class="conversation-composer__approval-actions">
        <Button size="sm" :disabled="running || cancelling" :agent-id="`${agentIdPrefix}.plan-approval.cancel`" @click="emit('decidePlan', 'cancel')">取消</Button>
        <Button size="sm" :disabled="running || cancelling || !planRevision.trim()" :agent-id="`${agentIdPrefix}.plan-approval.revise`" @click="emit('decidePlan', 'revise', planRevision.trim())">修改计划</Button>
        <Button variant="primary" size="sm" :disabled="running || cancelling" :agent-id="`${agentIdPrefix}.plan-approval.approve`" @click="emit('decidePlan', 'approve')">按计划执行</Button>
      </div>
    </div>

    <div
      v-else-if="pendingQuestion"
      class="conversation-composer__pending"
      :data-agent-id="`${agentIdPrefix}.ask`"
    >
      <p class="conversation-composer__question">{{ pendingQuestion.question }}</p>
      <div v-if="pendingQuestion.options.length" class="conversation-composer__options">
        <Button
          v-for="(option, index) in pendingQuestion.options"
          :key="option"
          size="sm"
          :agent-id="`${agentIdPrefix}.ask-option.${index}`"
          @click="emit('answer', option)"
        >
          {{ option }}
        </Button>
      </div>
      <div class="conversation-composer__answer">
        <Textarea
          ref="askRef"
          :model-value="askAnswer"
          :rows="1"
          placeholder="输入回答"
          :agent-id="`${agentIdPrefix}.ask-input`"
          @update:model-value="emit('update:askAnswer', $event)"
          @keydown="onAskKeydown"
        />
        <div class="conversation-composer__answer-actions">
          <Button size="sm" :agent-id="`${agentIdPrefix}.cancel`" @click="emit('cancel')">取消</Button>
          <Button variant="primary" size="sm" :disabled="!askAnswer.trim()" :agent-id="`${agentIdPrefix}.ask-submit`" @click="emit('answer')">回答</Button>
        </div>
      </div>
    </div>

    <div
      v-else-if="computerApproval"
      class="conversation-composer__approval"
      :data-agent-id="`${agentIdPrefix}.computer-approval`"
    >
      <div class="conversation-composer__approval-heading">
        <strong>授权 Agent 操作 Cubism</strong>
        <span>{{ computerApproval.targetWindowTitle }}</span>
      </div>
      <p>{{ computerApproval.reason }}</p>
      <p class="conversation-composer__approval-goal">{{ computerApproval.goal }}</p>
      <p>{{ computerApproval.impact }}</p>
      <ol>
        <li v-for="step in computerApproval.steps" :key="step.id">{{ step.title }}</li>
      </ol>
      <div class="conversation-composer__approval-meta">
        <span>{{ computerApproval.allowedActions.map((kind) => actionLabels[kind]).join("、") }}</span>
        <span>{{ computerApproval.includesFileDialogs ? "包含 Cubism 文件对话框" : "不包含文件对话框" }}</span>
        <span>{{ expiryLabel(computerApproval.expiresAt) }}</span>
        <span v-if="computerApproval.cannotUndo">取消只会停止后续输入，无法保证自动撤销</span>
      </div>
      <div class="conversation-composer__approval-actions">
        <Button
          size="sm"
          :agent-id="`${agentIdPrefix}.computer-approval.reject`"
          @click="emit('decide', false)"
        >
          拒绝
        </Button>
        <Button
          variant="primary"
          size="sm"
          :agent-id="`${agentIdPrefix}.computer-approval.approve`"
          @click="emit('decide', true)"
        >
          授权本次操作
        </Button>
      </div>
    </div>

    <template v-else>
      <div
        v-if="computerStatus !== 'idle'"
        class="conversation-composer__operation-status"
        :data-agent-id="`${agentIdPrefix}.computer-operation-status`"
      >
        <span class="conversation-composer__status-dot" />
        {{ statusLabels[computerStatus] }}
      </div>
      <Textarea
        ref="inputRef"
        :model-value="modelValue"
        :rows="1"
        :placeholder="mode === 'plan' ? '描述需要规划的 Cubism 工作…' : mode === 'conversation_only' ? '提问或讨论当前模型…' : placeholder"
        :disabled="disabled || running || cancelling"
        :agent-id="`${agentIdPrefix}.input`"
        @update:model-value="emit('update:modelValue', $event)"
        @keydown="onKeydown"
        @paste="emit('paste', $event)"
      />
      <div v-if="images.length" class="conversation-composer__images">
        <div
          v-for="(image, index) in images"
          :key="image.draftId"
          class="conversation-composer__image"
          :data-agent-id="`${agentIdPrefix}.draft-image.${index}`"
        >
          <button type="button" class="conversation-composer__image-open" :aria-label="`查看 ${image.name}`" @click="emit('viewImage', image)">
            <img :src="chatImageSrc(image)" :alt="image.name" />
          </button>
          <button
            type="button"
            class="conversation-composer__image-remove"
            :aria-label="`移除 ${image.name}`"
            :data-agent-id="`${agentIdPrefix}.draft-image.${index}.remove`"
            @click="emit('removeImage', image.draftId)"
          >
            <X :size="12" aria-hidden="true" />
          </button>
        </div>
      </div>
      <div class="conversation-composer__actions">
        <div class="conversation-composer__mode">
          <Button
            size="sm"
            variant="ghost"
            :title="imageInputDisabled ? '当前模型不支持图片输入，请更换支持视觉的模型' : '添加图片'"
            aria-label="添加图片"
            :disabled="disabled || running || cancelling || images.length >= MAX_CHAT_IMAGES || imageInputDisabled"
            :agent-id="`${agentIdPrefix}.add-image`"
            @click="emit('pickImages')"
          >
            <ImagePlus :size="15" aria-hidden="true" />
          </Button>
          <Button
            size="sm"
            :variant="mode === 'conversation_only' ? 'primary' : 'ghost'"
            :aria-pressed="mode === 'conversation_only'"
            :disabled="disabled || running || cancelling"
            :agent-id="`${agentIdPrefix}.conversation-only`"
            @click="emit('update:mode', mode === 'conversation_only' ? 'default' : 'conversation_only')"
          >
            仅对话
          </Button>
          <Button
            size="sm"
            :variant="mode === 'plan' ? 'primary' : 'ghost'"
            :aria-pressed="mode === 'plan'"
            :disabled="disabled || running || cancelling"
            :agent-id="`${agentIdPrefix}.plan-mode`"
            @click="emit('update:mode', mode === 'plan' ? 'default' : 'plan')"
          >
            <ListChecks :size="15" aria-hidden="true" />
            计划
          </Button>
        </div>
        <span class="conversation-composer__hint">Enter 发送 · Shift+Enter 换行</span>
        <Button
          v-if="running || cancelling"
          size="sm"
          :loading="cancelling"
          :agent-id="`${agentIdPrefix}.stop`"
          @click="emit('cancel')"
        >
          {{ cancelling ? "正在停止" : "停止" }}
        </Button>
        <Button
          v-else
          variant="primary"
          size="sm"
          :disabled="!canSend"
          :agent-id="`${agentIdPrefix}.send`"
          @click="emit('send')"
        >
          发送
        </Button>
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
.conversation-composer__images,
.conversation-composer__actions,
.conversation-composer__mode,
.conversation-composer__answer-actions,
.conversation-composer__options,
.conversation-composer__approval-actions,
.conversation-composer__operation-status { display: flex; align-items: center; gap: 6px; }
.conversation-composer__toolbar { min-height: 28px; padding: 0 1px 6px; overflow-x: auto; }
.conversation-composer__images { flex-wrap: wrap; padding: 5px 1px 1px; }
.conversation-composer__image { position: relative; width: 58px; height: 58px; }
.conversation-composer__image-open { width: 100%; height: 100%; padding: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-subtle); cursor: zoom-in; }
.conversation-composer__image-open img { display: block; width: 100%; height: 100%; object-fit: cover; }
.conversation-composer__image-remove { position: absolute; top: -5px; right: -5px; display: grid; place-items: center; width: 18px; height: 18px; padding: 0; border: 1px solid var(--border); border-radius: 50%; background: var(--bg-elev); color: var(--text); cursor: pointer; }
.conversation-composer :deep(.ui-textarea) { display: block; width: 100%; min-height: 48px; max-height: 180px; resize: none; overflow-y: auto; border: 0; background: transparent; box-shadow: none; line-height: 1.55; }
.conversation-composer :deep(.ui-textarea:focus) { outline: none; }
.conversation-composer__actions { justify-content: flex-end; min-height: 30px; padding: 6px 1px 0; }
.conversation-composer__mode { margin-right: auto; min-width: 0; }
.conversation-composer__hint { color: var(--text-faint); font-size: 11px; }
.conversation-composer__pending { min-height: 108px; padding: 3px; }
.conversation-composer__plan-approval { min-height: 0; }
.conversation-composer__plan-approval :deep(.ui-textarea) { margin-top: 7px; min-height: 42px; }
.conversation-composer__question { margin: 0 0 9px; font-size: 13px; line-height: 1.5; }
.conversation-composer__options { flex-wrap: wrap; margin-bottom: 7px; }
.conversation-composer__answer { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: end; gap: 7px; }
.conversation-composer__answer-actions { padding-bottom: 1px; }
.conversation-composer__error { margin: 6px 3px 0; color: var(--err); font-size: 12px; }
.conversation-composer__approval { padding: 3px; }
.conversation-composer__approval-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
.conversation-composer__approval-heading strong { font-size: 13px; }
.conversation-composer__approval-heading span,
.conversation-composer__approval-meta { color: var(--text-faint); font-size: 11px; }
.conversation-composer__approval p { margin: 7px 0 0; font-size: 12px; line-height: 1.45; }
.conversation-composer__approval-goal { color: var(--text); font-weight: 600; }
.conversation-composer__approval ol { margin: 6px 0; padding-left: 20px; font-size: 12px; line-height: 1.5; }
.conversation-composer__approval-meta { display: flex; flex-wrap: wrap; gap: 4px 12px; }
.conversation-composer__approval-actions { justify-content: flex-end; margin-top: 9px; }
.conversation-composer__operation-status { margin: 0 2px 6px; color: var(--text-muted); font-size: 11px; }
.conversation-composer__status-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }

@media (max-width: 620px) {
  .conversation-composer__hint { display: none; }
  .conversation-composer__answer { grid-template-columns: 1fr; }
  .conversation-composer__answer-actions { justify-content: flex-end; }
}
</style>
