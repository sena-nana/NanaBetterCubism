import { computed, reactive } from "vue";
import {
  getMessages,
  getPendingUserAction,
  getPlan,
  listPsds,
  listenComputerOperation,
  listenPlan,
  listenToolEvent,
  listenTurnDelta,
  listenTurnFinished,
  listenUserAction,
  normalizeCommandError,
} from "./bridge";
import type {
  AgentToolEvent,
  AgentTurnMode,
  ChatMessage,
  ChatImageDraft,
  ChatPsdDocument,
  ComputerOperationStatus,
  ConversationPlan,
  PendingUserAction,
} from "./types";

export type ConversationTurnPhase = "idle" | "running" | "awaiting_input" | "cancelling";

export interface ConversationRuntimeState {
  conversationId: string;
  phase: ConversationTurnPhase;
  messages: ChatMessage[];
  plan: ConversationPlan | null;
  pendingAction: PendingUserAction | null;
  computerStatus: ComputerOperationStatus;
  draft: string;
  imageDrafts: ChatImageDraft[];
  psdDocuments: ChatPsdDocument[];
  askAnswer: string;
  planRevision: string;
  composerMode: AgentTurnMode;
  error: string | null;
  loading: boolean;
  loaded: boolean;
}

interface InternalConversationRuntimeState extends ConversationRuntimeState {
  loadEpoch: number;
  revision: number;
}

const states = reactive<Record<string, InternalConversationRuntimeState>>({});
const phaseListeners = new Set<() => void>();
let installPromise: Promise<void> | null = null;
let localSequence = 0;

export function installConversationRuntimeStore() {
  if (installPromise) return installPromise;
  installPromise = Promise.all([
    listenTurnDelta((payload) => appendDelta(payload.conversationId, payload.text)),
    listenToolEvent((payload) => upsertToolEvent(payload.conversationId, payload)),
    listenTurnFinished((payload) => {
      const state = runtimeState(payload.conversationId);
      state.error = payload.ok ? null : payload.message;
      setConversationTurnPhase(payload.conversationId, "idle");
      touch(state);
      void loadConversationRuntime(payload.conversationId, {
        force: true,
        preserveError: !payload.ok,
      }).catch((error) => {
        if (payload.ok) state.error = normalizeCommandError(error).message;
      });
    }),
    listenUserAction((payload) => {
      const state = runtimeState(payload.conversationId);
      state.pendingAction = payload.action;
      state.askAnswer = "";
      if (payload.action.kind === "plan_approval") {
        state.composerMode = "plan";
        state.planRevision = "";
      }
      if (payload.action.kind === "computer_approval") {
        state.computerStatus = "awaiting_approval";
      }
      setConversationTurnPhase(payload.conversationId, "awaiting_input");
      touch(state);
    }),
    listenComputerOperation((payload) => {
      const state = runtimeState(payload.conversationId);
      state.computerStatus = payload.status;
      touch(state);
    }),
    listenPlan((payload) => {
      const state = runtimeState(payload.conversationId);
      state.plan = payload.plan;
      touch(state);
    }),
  ])
    .then(() => undefined)
    .catch((error) => {
      installPromise = null;
      throw error;
    });
  return installPromise;
}

export function useConversationRuntime(conversationId: () => string) {
  const state = computed(() => runtimeState(conversationId()));
  const phase = computed(() => state.value.phase);
  return {
    state,
    running: computed(() => phase.value === "running"),
    cancelling: computed(() => phase.value === "cancelling"),
    blocked: computed(() => phase.value !== "idle"),
  };
}

export function getConversationRuntime(conversationId: string): ConversationRuntimeState {
  return runtimeState(conversationId);
}

export async function loadConversationRuntime(
  conversationId: string,
  options: { force?: boolean; preserveError?: boolean } = {},
) {
  const state = runtimeState(conversationId);
  if (state.loaded && !options.force) return state;
  const epoch = ++state.loadEpoch;
  const revision = state.revision;
  state.loading = true;
  if (!options.preserveError) state.error = null;
  try {
    const [messages, plan, pendingAction, psdDocuments] = await Promise.all([
      getMessages(conversationId),
      getPlan(conversationId),
      getPendingUserAction(conversationId),
      listPsds(conversationId),
    ]);
    if (epoch !== state.loadEpoch) return state;

    const changedWhileLoading = revision !== state.revision;
    const keepTransient = state.phase !== "idle" || changedWhileLoading;
    state.messages = keepTransient
      ? mergePersistedAndTransientMessages(messages, state.messages)
      : [...messages];
    if (!changedWhileLoading) {
      state.plan = plan;
      state.pendingAction = pendingAction;
      state.psdDocuments = psdDocuments;
      if (pendingAction?.kind === "plan_approval") state.composerMode = "plan";
      if (pendingAction?.kind === "computer_approval") {
        state.computerStatus = "awaiting_approval";
      }
    }
    if (pendingAction) setConversationTurnPhase(conversationId, "awaiting_input");
    state.loaded = true;
    return state;
  } finally {
    if (epoch === state.loadEpoch) state.loading = false;
  }
}

export function beginConversationTurn(
  conversationId: string,
  content: string,
  imageDrafts: ChatImageDraft[] = [],
) {
  const state = runtimeState(conversationId);
  const messageId = nextLocalId("local");
  state.draft = "";
  state.imageDrafts = [];
  state.error = null;
  state.computerStatus = "idle";
  state.messages.push({
    id: messageId,
    role: "user",
    content,
    toolName: null,
    toolDisplayName: null,
    toolStatus: null,
    attachments: imageDrafts.map(({ draftId: _, ...attachment }) => attachment),
    createdAt: new Date().toISOString(),
  });
  setConversationTurnPhase(conversationId, "running");
  touch(state);
  return messageId;
}

export function failConversationTurn(
  conversationId: string,
  optimisticMessageId: string,
  content: string,
  imageDrafts: ChatImageDraft[],
  message: string,
) {
  const state = runtimeState(conversationId);
  state.messages = state.messages.filter((item) => item.id !== optimisticMessageId);
  state.draft = content;
  state.imageDrafts = imageDrafts;
  state.error = message;
  setConversationTurnPhase(conversationId, "idle");
  touch(state);
}

export function confirmConversationTurn(
  conversationId: string,
  optimisticMessageId: string,
  persisted: ChatMessage,
) {
  const state = runtimeState(conversationId);
  const index = state.messages.findIndex((message) => message.id === optimisticMessageId);
  if (index >= 0) state.messages.splice(index, 1, persisted);
  touch(state);
}

export function setConversationTurnPhase(
  conversationId: string,
  phase: ConversationTurnPhase,
) {
  if (!conversationId) return;
  const state = runtimeState(conversationId);
  if (state.phase === phase) return;
  state.phase = phase;
  touch(state);
  for (const listener of phaseListeners) listener();
}

export function getConversationTurnPhase(conversationId: string): ConversationTurnPhase {
  return states[conversationId]?.phase ?? "idle";
}

export function clearConversationTurnPhase(conversationId: string) {
  if (!(conversationId in states)) return;
  delete states[conversationId];
  for (const listener of phaseListeners) listener();
}

export function subscribeConversationTurnPhases(listener: () => void) {
  phaseListeners.add(listener);
  return () => phaseListeners.delete(listener);
}

function runtimeState(conversationId: string) {
  if (!states[conversationId]) {
    states[conversationId] = {
      conversationId,
      phase: "idle",
      messages: [],
      plan: null,
      pendingAction: null,
      computerStatus: "idle",
      draft: "",
      imageDrafts: [],
      psdDocuments: [],
      askAnswer: "",
      planRevision: "",
      composerMode: "default",
      error: null,
      loading: true,
      loaded: false,
      loadEpoch: 0,
      revision: 0,
    };
  }
  return states[conversationId];
}

function appendDelta(conversationId: string, text: string) {
  const state = runtimeState(conversationId);
  const last = state.messages[state.messages.length - 1];
  if (last?.role === "assistant" && last.id.startsWith("stream-")) {
    last.content += text;
  } else {
    state.messages.push({
      id: nextLocalId("stream"),
      role: "assistant",
      content: text,
      toolName: null,
      toolDisplayName: null,
      toolStatus: null,
      attachments: [],
      createdAt: new Date().toISOString(),
    });
  }
  touch(state);
}

function upsertToolEvent(conversationId: string, payload: AgentToolEvent) {
  const state = runtimeState(conversationId);
  const messageId = `tool-${payload.toolCallId}`;
  const active = state.messages.find((message) => message.id === messageId);
  if (active) {
    active.toolStatus = payload.status;
    active.content = payload.summary;
    active.toolDisplayName = payload.toolDisplayName;
  } else {
    state.messages.push({
      id: messageId,
      role: "tool",
      content: payload.summary,
      toolName: payload.toolName,
      toolDisplayName: payload.toolDisplayName,
      toolStatus: payload.status,
      attachments: [],
      createdAt: new Date().toISOString(),
    });
  }
  touch(state);
}

function mergePersistedAndTransientMessages(
  persisted: ChatMessage[],
  current: ChatMessage[],
) {
  const transient = current.filter((message) =>
    message.id.startsWith("local-") ||
    message.id.startsWith("stream-") ||
    message.id.startsWith("tool-")
  );
  const lastPersistedUser = [...persisted].reverse().find((message) => message.role === "user");
  return [
    ...persisted,
    ...transient.filter(
      (message) =>
        !message.id.startsWith("local-") ||
        message.content !== lastPersistedUser?.content,
    ),
  ];
}

function nextLocalId(prefix: string) {
  return `${prefix}-${Date.now()}-${localSequence++}`;
}

function touch(state: InternalConversationRuntimeState) {
  state.revision += 1;
}
