<script setup lang="ts">
import { UiButton } from "@lilia/ui";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useEditorStore } from "../editor/editorStore";
import {
  answerAsk,
  cancelTurn,
  consolidateMemory,
  getLlmConfig,
  getMessages,
  getPendingAsk,
  getPlan,
  listConversations,
  listenAsk,
  listenPlan,
  listenToolEvent,
  listenTurnDelta,
  listenTurnFinished,
  normalizeCommandError,
  sendMessage,
} from "./bridge";
import ConversationComposer from "./components/ConversationComposer.vue";
import PlanTodoPanel from "./components/PlanTodoPanel.vue";
import ConversationSurface from "./components/ConversationSurface.vue";
import ConversationTranscript from "./components/ConversationTranscript.vue";
import { editorStatusLabel, modelStatusLabel } from "./conversationPresentation";
import {
  setConversationTurnPhase,
  useConversationTurn,
} from "./conversationRuntimeStore";
import {
  applyConversationGroup,
} from "./sidebarConversations";
import type {
  AgentToolEvent,
  ChatMessage,
  ConversationPlan,
  ConversationSummary,
  LlmConfigView,
  PendingAsk,
} from "./types";

const route = useRoute();
const router = useRouter();
const editor = useEditorStore();
const conversationId = computed(() => String(route.params.id ?? ""));
const turn = useConversationTurn(() => conversationId.value);

const messages = ref<ChatMessage[]>([]);
const conversation = ref<ConversationSummary | null>(null);
const plan = ref<ConversationPlan | null>(null);
const pendingAsk = ref<PendingAsk | null>(null);
const draft = ref("");
const askAnswer = ref("");
const loading = ref(true);
const consolidating = ref(false);
const error = ref<string | null>(null);
const llm = ref<LlmConfigView>({ baseUrl: null, model: null, hasApiKey: false });

const unlisteners: Array<() => void> = [];
let loadEpoch = 0;
let localSequence = 0;
let disposed = false;

const canSend = computed(
  () =>
    Boolean(draft.value.trim()) &&
    llm.value.hasApiKey &&
    !turn.blocked.value &&
    !pendingAsk.value,
);

const currentProjectName = computed(() => conversation.value?.projectName?.trim() || "收集箱");

onMounted(async () => {
  disposed = false;
  void editor.initialize();
  await installListeners();
  await reloadConversation();
});

onUnmounted(() => {
  disposed = true;
  loadEpoch += 1;
  for (const stop of unlisteners) stop();
});

watch(conversationId, () => {
  void reloadConversation();
});

async function installListeners() {
  const listeners = await Promise.all([
    listenTurnDelta((payload) => {
      if (payload.conversationId !== conversationId.value) return;
      appendDelta(payload.text);
    }),
    listenToolEvent((payload) => {
      if (payload.conversationId !== conversationId.value) return;
      upsertToolEvent(payload);
    }),
    listenTurnFinished(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      if (!payload.ok) error.value = payload.message;
      await reloadConversation({ preserveError: !payload.ok });
    }),
    listenAsk((payload) => {
      if (payload.conversationId !== conversationId.value) return;
      pendingAsk.value = payload.ask;
      askAnswer.value = "";
    }),
    listenPlan((payload) => {
      if (payload.conversationId !== conversationId.value) return;
      plan.value = payload.plan;
    }),
  ]);
  if (disposed) {
    for (const stop of listeners) stop();
    return;
  }
  unlisteners.push(...listeners);
}

async function reloadConversation(options: { preserveError?: boolean } = {}) {
  const id = conversationId.value;
  const epoch = ++loadEpoch;
  if (!id) return;
  loading.value = true;
  if (!options.preserveError) error.value = null;
  try {
    const [nextMessages, nextPlan, nextAsk, conversations, nextLlm] =
      await Promise.all([
        getMessages(id),
        getPlan(id),
        getPendingAsk(id),
        listConversations(),
        getLlmConfig(),
      ]);
    if (disposed || epoch !== loadEpoch || id !== conversationId.value) return;
    messages.value = nextMessages;
    plan.value = nextPlan;
    pendingAsk.value = nextAsk;
    llm.value = nextLlm;
    applyConversationGroup(conversations);
    conversation.value = conversations.find((item) => item.id === id) ?? null;
    if (nextAsk) setConversationTurnPhase(id, "awaiting_input");
  } catch (err) {
    if (!disposed && epoch === loadEpoch && id === conversationId.value) {
      const normalized = normalizeCommandError(err);
      if (normalized.code === "not_found") {
        await router.replace("/");
        return;
      }
      error.value = normalized.message;
    }
  } finally {
    if (!disposed && epoch === loadEpoch && id === conversationId.value) {
      loading.value = false;
    }
  }
}

function appendDelta(text: string) {
  const last = messages.value[messages.value.length - 1];
  if (last?.role === "assistant" && last.id.startsWith("stream-")) {
    last.content += text;
    return;
  }
  messages.value.push({
    id: `stream-${Date.now()}-${localSequence++}`,
    role: "assistant",
    content: text,
    toolName: null,
    toolStatus: null,
    createdAt: new Date().toISOString(),
  });
}

function upsertToolEvent(payload: AgentToolEvent) {
  const active = [...messages.value]
    .reverse()
    .find(
      (message) =>
        message.role === "tool" &&
        message.toolName === payload.toolName &&
        message.toolStatus === "started",
    );
  if (active && payload.status !== "started") {
    active.toolStatus = payload.status;
    active.content = payload.summary;
    return;
  }
  messages.value.push({
    id: `tool-${Date.now()}-${localSequence++}`,
    role: "tool",
    content: payload.summary,
    toolName: payload.toolName,
    toolStatus: payload.status,
    createdAt: new Date().toISOString(),
  });
}

async function onSend() {
  const id = conversationId.value;
  const content = draft.value.trim();
  if (!id || !content || !canSend.value) return;
  const optimisticId = `local-${Date.now()}-${localSequence++}`;
  draft.value = "";
  error.value = null;
  messages.value.push({
    id: optimisticId,
    role: "user",
    content,
    toolName: null,
    toolStatus: null,
    createdAt: new Date().toISOString(),
  });
  setConversationTurnPhase(id, "running");
  try {
    await sendMessage(id, content);
  } catch (err) {
    setConversationTurnPhase(id, "idle");
    messages.value = messages.value.filter((message) => message.id !== optimisticId);
    draft.value = content;
    error.value = normalizeCommandError(err).message;
  }
}

async function onCancel() {
  const id = conversationId.value;
  if (!id) return;
  error.value = null;
  try {
    const result = await cancelTurn(id);
    if (result.state === "cancel_requested") {
      setConversationTurnPhase(id, "cancelling");
      return;
    }
    pendingAsk.value = null;
    askAnswer.value = "";
    setConversationTurnPhase(id, "idle");
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  }
}

async function onAnswerAsk(answer?: string) {
  const currentAsk = pendingAsk.value;
  const value = (answer ?? askAnswer.value).trim();
  if (!currentAsk || !value) return;
  error.value = null;
  try {
    await answerAsk(currentAsk.askId, value);
    pendingAsk.value = null;
    askAnswer.value = "";
    setConversationTurnPhase(currentAsk.conversationId, "running");
  } catch (err) {
    setConversationTurnPhase(currentAsk.conversationId, "awaiting_input");
    error.value = normalizeCommandError(err).message;
  }
}

async function onConsolidate() {
  const id = conversationId.value;
  if (!id) return;
  consolidating.value = true;
  error.value = null;
  try {
    await consolidateMemory(id);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    consolidating.value = false;
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
        <div class="conversation-header__actions">
          <UiButton
            size="sm"
            :busy="consolidating"
            agent-id="agent.chat.consolidate"
            @click="onConsolidate"
          >
            整理记忆
          </UiButton>
        </div>
      </div>
    </template>

    <ConversationTranscript
      :messages="messages"
      :loading="loading"
      :running="turn.running.value"
      :cancelling="turn.cancelling.value"
      :empty-title="`要在${currentProjectName === '收集箱' ? '' : ` ${currentProjectName} `}完成什么？`"
    />

    <template #context>
      <PlanTodoPanel v-if="plan?.steps.length" :plan="plan" data-agent-id="agent.chat.plan" />
    </template>

    <template #composer>
      <ConversationComposer
        v-model="draft"
        v-model:ask-answer="askAnswer"
        :pending-ask="pendingAsk"
        :disabled="!llm.hasApiKey"
        :running="turn.running.value"
        :cancelling="turn.cancelling.value"
        :can-send="canSend"
        :error="error"
        @send="onSend"
        @cancel="onCancel"
        @answer="onAnswerAsk"
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
  justify-content: space-between;
  gap: 12px;
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

.conversation-header__actions {
  display: flex;
  align-items: center;
  gap: 6px;
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
