<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import UiImageViewer from "@lilia/image-viewer/components/ImageViewer";
import {
  answerQuestion,
  cancelTurn,
  decidePlan,
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
  confirmConversationTurn,
  getConversationRuntime,
  installConversationRuntimeStore,
  loadConversationRuntime,
  setConversationTurnPhase,
  useConversationRuntime,
} from "./conversationRuntimeStore";
import {
  applyConversationGroup,
} from "./sidebarConversations";
import type { ConversationSummary, PlanDecision } from "./types";
import { useChatImageDrafts } from "./useChatImageDrafts";
import { useChatPsdDocuments } from "./useChatPsdDocuments";

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
const planRevision = computed({
  get: () => runtime.value.planRevision,
  set: (value: string) => {
    runtime.value.planRevision = value;
  },
});
const composerMode = computed({
  get: () => runtime.value.composerMode,
  set: (mode) => {
    runtime.value.composerMode = mode;
  },
});
const imageDrafts = computed({
  get: () => runtime.value.imageDrafts,
  set: (value) => {
    runtime.value.imageDrafts = value;
  },
});
const canCompose = computed(
  () => llm.state.config.hasApiKey && !turn.blocked.value && !runtime.value.pendingAction,
);

const imageInputDisabled = computed(() => llm.state.imageInputSupported === false);
const canDropImages = computed(() => canCompose.value && !imageInputDisabled.value);

const imageDraftController = useChatImageDrafts({
  drafts: imageDrafts,
  canInteract: () => canCompose.value,
  setError: (message) => {
    runtime.value.error = message;
  },
});
const viewingImage = imageDraftController.viewingImage;

const psdDocuments = computed({
  get: () => runtime.value.psdDocuments,
  set: (value) => {
    runtime.value.psdDocuments = value;
  },
});
const psdController = useChatPsdDocuments({
  conversationId: () => conversationId.value,
  documents: psdDocuments,
  canInteract: () => canCompose.value,
  setError: (message) => {
    runtime.value.error = message;
  },
});

const canSend = computed(
  () =>
    Boolean(draft.value.trim() || imageDrafts.value.length) &&
    canCompose.value,
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
  if (!id || !canSend.value) return;
  const images = [...imageDrafts.value];
  const optimisticId = beginConversationTurn(id, content, images);
  try {
    const persisted = await sendMessage(
      id,
      content,
      images.map((image) => image.draftId),
      runtime.value.composerMode,
    );
    confirmConversationTurn(id, optimisticId, persisted);
  } catch (err) {
    failConversationTurn(
      id,
      optimisticId,
      content,
      images,
      normalizeCommandError(err).message,
    );
  }
}

async function onDecidePlan(decision: PlanDecision, revision?: string) {
  const approval = runtime.value.pendingAction?.kind === "plan_approval"
    ? runtime.value.pendingAction
    : null;
  if (!approval) return;
  const state = getConversationRuntime(approval.conversationId);
  state.error = null;
  setConversationTurnPhase(approval.conversationId, "running");
  try {
    const result = await decidePlan(approval.actionId, decision, revision);
    state.pendingAction = null;
    state.planRevision = "";
    state.composerMode = result === "execution_started" ? "default" : "plan";
    setConversationTurnPhase(
      approval.conversationId,
      result === "cancelled" ? "idle" : "running",
    );
  } catch (err) {
    state.pendingAction = approval;
    setConversationTurnPhase(approval.conversationId, "awaiting_input");
    state.error = normalizeCommandError(err).message;
  }
}

async function onCancel() {
  const id = conversationId.value;
  if (!id) return;
  const state = getConversationRuntime(id);
  const wasComputerOperation =
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

</script>

<template>
  <ConversationSurface
    data-agent-id="agent.chat"
    :drop-enabled="canDropImages"
    @drop-paths="imageDraftController.addPaths"
  >
    <ConversationTranscript
      :messages="runtime.messages"
      :loading="runtime.loading"
      :running="turn.running.value"
      :cancelling="turn.cancelling.value"
      :empty-title="`要在${currentProjectName === '收集箱' ? '' : ` ${currentProjectName} `}完成什么？`"
      @view-image="imageDraftController.viewImage"
    />

    <template #context>
      <PlanTodoPanel
        v-if="runtime.plan?.steps.length"
        :plan="runtime.plan"
        data-agent-id="agent.chat.plan"
      />
    </template>

    <template #composer>
      <p
        v-if="imageInputDisabled"
        class="chat-image-unsupported-banner"
        role="alert"
        data-agent-id="agent.chat.image-unsupported"
      >
        当前模型不支持图片输入，「查看 Editor 窗口」等能力已禁用。
        <button type="button" class="chat-image-unsupported-banner__link" @click="router.push('/settings?tab=model-config')">
          更换模型
        </button>
      </p>
      <ConversationComposer
        v-model="draft"
        v-model:ask-answer="askAnswer"
        v-model:plan-revision="planRevision"
        v-model:mode="composerMode"
        :pending-action="runtime.pendingAction"
        :computer-status="runtime.computerStatus"
        :disabled="!llm.state.config.hasApiKey"
        :running="turn.running.value"
        :cancelling="turn.cancelling.value"
        :can-send="canSend"
        :images="imageDrafts"
        :psd-documents="psdDocuments"
        :error="runtime.error"
        :image-input-disabled="imageInputDisabled"
        @send="onSend"
        @cancel="onCancel"
        @answer="onAnswerAsk"
        @decide-plan="onDecidePlan"
        @pick-images="imageDraftController.pickImages"
        @pick-psd="psdController.pickPsd"
        @remove-image="imageDraftController.removeImage"
        @remove-psd="psdController.removePsd"
        @view-image="imageDraftController.viewImage"
        @paste="imageDraftController.pasteImages"
      />
    </template>
  </ConversationSurface>
  <UiImageViewer
    v-if="viewingImage"
    :source="viewingImage"
    agent-id="agent.chat.image-viewer"
    @close="viewingImage = null"
  />
</template>

<style scoped>
.chat-image-unsupported-banner {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin: 0 3px 6px;
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-subtle);
  color: var(--text);
  font-size: 12px;
  line-height: 1.5;
}

.chat-image-unsupported-banner__link {
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-elev);
  color: var(--text);
  font-size: 12px;
  cursor: pointer;
}

.chat-image-unsupported-banner__link:hover {
  border-color: var(--text);
}
</style>
