<script setup lang="ts">
import { UiButton } from "@lilia/ui";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useEditorStore } from "../editor/editorStore";
import {
  answerQuestion,
  cancelTurn,
  decideComputerOperation,
  getLlmConfig,
  listConversations,
  normalizeCommandError,
  sendMessage,
} from "./bridge";
import ConversationComposer from "./components/ConversationComposer.vue";
import PlanTodoPanel from "./components/PlanTodoPanel.vue";
import ConversationSurface from "./components/ConversationSurface.vue";
import ConversationTranscript from "./components/ConversationTranscript.vue";
import { editorStatusLabel, modelStatusLabel } from "./conversationPresentation";
import {
  beginConversationTurn,
  failConversationTurn,
  getConversationRuntime,
  installConversationRuntimeStore,
  loadConversationRuntime,
  setConversationTurnPhase,
  useConversationRuntime,
} from "./conversationRuntimeStore";
import {
  applyConversationGroup,
} from "./sidebarConversations";
import type {
  ConversationSummary,
  LlmConfigView,
} from "./types";

const route = useRoute();
const router = useRouter();
const editor = useEditorStore();
const conversationId = computed(() => String(route.params.id ?? ""));
const turn = useConversationRuntime(() => conversationId.value);
const runtime = turn.state;

const conversation = ref<ConversationSummary | null>(null);
const llm = ref<LlmConfigView>({ baseUrl: null, model: null, hasApiKey: false });

let loadEpoch = 0;
let disposed = false;

const draft = computed({
  get: () => runtime.value.draft,
  set: (value: string) => {
    runtime.value.draft = value;
  },
});
const askAnswer = computed({
  get: () => runtime.value.askAnswer,
  set: (value: string) => {
    runtime.value.askAnswer = value;
  },
});

const canSend = computed(
  () =>
    Boolean(draft.value.trim()) &&
    llm.value.hasApiKey &&
    !turn.blocked.value &&
    !runtime.value.pendingAction,
);

const currentProjectName = computed(() => conversation.value?.projectName?.trim() || "收集箱");

onMounted(async () => {
  disposed = false;
  void editor.initialize();
  await installConversationRuntimeStore();
  await reloadConversation();
});

onUnmounted(() => {
  disposed = true;
  loadEpoch += 1;
});

watch(conversationId, () => {
  void reloadConversation();
});

async function reloadConversation() {
  const id = conversationId.value;
  const epoch = ++loadEpoch;
  if (!id) return;
  const state = getConversationRuntime(id);
  state.error = null;
  try {
    const [, conversations, nextLlm] =
      await Promise.all([
        loadConversationRuntime(id),
        listConversations(),
        getLlmConfig(),
      ]);
    if (disposed || epoch !== loadEpoch || id !== conversationId.value) return;
    llm.value = nextLlm;
    applyConversationGroup(conversations);
    conversation.value = conversations.find((item) => item.id === id) ?? null;
  } catch (err) {
    if (!disposed && epoch === loadEpoch && id === conversationId.value) {
      const normalized = normalizeCommandError(err);
      if (normalized.code === "not_found") {
        await router.replace("/");
        return;
      }
      state.error = normalized.message;
    }
  }
}

async function onSend() {
  const id = conversationId.value;
  const content = draft.value.trim();
  if (!id || !content || !canSend.value) return;
  const optimisticId = beginConversationTurn(id, content);
  try {
    await sendMessage(id, content);
  } catch (err) {
    failConversationTurn(
      id,
      optimisticId,
      content,
      normalizeCommandError(err).message,
    );
  }
}

async function onCancel() {
  const id = conversationId.value;
  if (!id) return;
  const state = getConversationRuntime(id);
  const wasComputerOperation =
    state.pendingAction?.kind === "computer_approval" ||
    !["idle", "completed", "cancelled", "failed"].includes(state.computerStatus);
  state.error = null;
  try {
    const result = await cancelTurn(id);
    if (result.state === "cancel_requested") {
      setConversationTurnPhase(id, "cancelling");
      return;
    }
    state.pendingAction = null;
    state.askAnswer = "";
    if (wasComputerOperation) state.computerStatus = "cancelled";
    setConversationTurnPhase(id, "idle");
  } catch (err) {
    state.error = normalizeCommandError(err).message;
  }
}

async function onAnswerAsk(answer?: string) {
  const currentAsk = runtime.value.pendingAction?.kind === "question"
    ? runtime.value.pendingAction
    : null;
  const value = (answer ?? askAnswer.value).trim();
  if (!currentAsk || !value) return;
  const state = getConversationRuntime(currentAsk.conversationId);
  state.error = null;
  try {
    await answerQuestion(currentAsk.actionId, value);
    state.pendingAction = null;
    state.askAnswer = "";
    setConversationTurnPhase(currentAsk.conversationId, "running");
  } catch (err) {
    setConversationTurnPhase(currentAsk.conversationId, "awaiting_input");
    state.error = normalizeCommandError(err).message;
  }
}

async function onDecideComputerOperation(approved: boolean) {
  const approval = runtime.value.pendingAction?.kind === "computer_approval"
    ? runtime.value.pendingAction
    : null;
  if (!approval) return;
  const state = getConversationRuntime(approval.conversationId);
  state.error = null;
  try {
    await decideComputerOperation(approval.actionId, approved);
    state.pendingAction = null;
    state.computerStatus = approved ? "authorized" : "cancelled";
    setConversationTurnPhase(approval.conversationId, "running");
  } catch (err) {
    setConversationTurnPhase(approval.conversationId, "awaiting_input");
    state.error = normalizeCommandError(err).message;
  }
}

function goSettings(tab: string) {
  void router.push(`/settings?tab=${tab}`);
}

</script>

<template>
  <ConversationSurface data-agent-id="agent.chat">
    <template #header>
      <div class="conversation-header" data-agent-id="agent.chat.header">
        <div class="conversation-header__identity">
          <h1>{{ conversation?.title || "对话" }}</h1>
          <p>{{ currentProjectName }}</p>
        </div>
      </div>
    </template>

    <ConversationTranscript
      :messages="runtime.messages"
      :loading="runtime.loading"
      :running="turn.running.value"
      :cancelling="turn.cancelling.value"
      :empty-title="`要在${currentProjectName === '收集箱' ? '' : ` ${currentProjectName} `}完成什么？`"
    />

    <template #context>
      <PlanTodoPanel
        v-if="runtime.plan?.steps.length"
        :plan="runtime.plan"
        data-agent-id="agent.chat.plan"
      />
    </template>

    <template #composer>
      <ConversationComposer
        v-model="draft"
        v-model:ask-answer="askAnswer"
        :pending-action="runtime.pendingAction"
        :computer-status="runtime.computerStatus"
        :disabled="!llm.hasApiKey"
        :running="turn.running.value"
        :cancelling="turn.cancelling.value"
        :can-send="canSend"
        :error="runtime.error"
        @send="onSend"
        @cancel="onCancel"
        @answer="onAnswerAsk"
        @decide="onDecideComputerOperation"
      >
        <template #toolbar>
          <span class="composer-toolbar__spacer" />
          <UiButton
            size="sm"
            agent-id="agent.chat.open-model-settings"
            @click="goSettings('model-config')"
          >
            {{ modelStatusLabel(llm) }}
          </UiButton>
          <UiButton
            size="sm"
            agent-id="agent.chat.open-editor-settings"
            @click="goSettings('editor')"
          >
            {{ editorStatusLabel(editor.state.snapshot.state) }}
          </UiButton>
        </template>
      </ConversationComposer>
    </template>
  </ConversationSurface>
</template>

<style scoped>
.conversation-header {
  display: flex;
  align-items: center;
  min-height: 52px;
  padding: 8px clamp(16px, 5vw, 64px);
}

.conversation-header__identity {
  min-width: 0;
}

.conversation-header__identity h1,
.conversation-header__identity p {
  overflow: hidden;
  margin: 0;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conversation-header__identity h1 {
  font-size: 15px;
  font-weight: 600;
}

.conversation-header__identity p {
  margin-top: 2px;
  color: var(--text-faint);
  font-size: 11px;
}

.composer-toolbar__spacer {
  flex: 1;
}

@media (max-width: 720px) {
  .conversation-header {
    padding-inline: 12px;
  }
}
</style>
