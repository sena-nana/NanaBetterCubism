<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  answerQuestion,
  cancelTurn,
  decideComputerOperation,
  listConversations,
  normalizeCommandError,
  sendMessage,
} from "./bridge";
import ConversationComposer from "./components/ConversationComposer.vue";
import PlanTodoPanel from "./components/PlanTodoPanel.vue";
import ConversationSurface from "./components/ConversationSurface.vue";
import ConversationTranscript from "./components/ConversationTranscript.vue";
import { useLlmConfigStore } from "./llmConfigStore";
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
import type { ConversationSummary } from "./types";

const route = useRoute();
const router = useRouter();
const llm = useLlmConfigStore();
const conversationId = computed(() => String(route.params.id ?? ""));
const turn = useConversationRuntime(() => conversationId.value);
const runtime = turn.state;

const conversation = ref<ConversationSummary | null>(null);

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
    llm.state.config.hasApiKey &&
    !turn.blocked.value &&
    !runtime.value.pendingAction,
);

const currentProjectName = computed(() => conversation.value?.projectName?.trim() || "收集箱");

onMounted(async () => {
  disposed = false;
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
    const [, conversations] =
      await Promise.all([
        loadConversationRuntime(id),
        listConversations(),
        llm.initialize(),
      ]);
    if (disposed || epoch !== loadEpoch || id !== conversationId.value) return;
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

</script>

<template>
  <ConversationSurface data-agent-id="agent.chat">
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
        :disabled="!llm.state.config.hasApiKey"
        :running="turn.running.value"
        :cancelling="turn.cancelling.value"
        :can-send="canSend"
        :error="runtime.error"
        @send="onSend"
        @cancel="onCancel"
        @answer="onAnswerAsk"
        @decide="onDecideComputerOperation"
      />
    </template>
  </ConversationSurface>
</template>
