import { computed, reactive } from "vue";
import { listenAsk, listenTurnFinished } from "./bridge";

type ConversationTurnPhase = "idle" | "running" | "awaiting_input" | "cancelling";

const phases = reactive<Record<string, ConversationTurnPhase>>({});
let installPromise: Promise<void> | null = null;

export function installConversationRuntimeStore() {
  if (installPromise) return installPromise;
  installPromise = Promise.all([
    listenAsk((payload) => setConversationTurnPhase(payload.conversationId, "awaiting_input")),
    listenTurnFinished((payload) => setConversationTurnPhase(payload.conversationId, "idle")),
  ]).then(() => undefined);
  return installPromise;
}

export function useConversationTurn(conversationId: () => string) {
  const phase = computed(() => phases[conversationId()] ?? "idle");
  return {
    running: computed(() => phase.value === "running"),
    cancelling: computed(() => phase.value === "cancelling"),
    blocked: computed(() => phase.value !== "idle"),
  };
}

export function setConversationTurnPhase(
  conversationId: string,
  phase: ConversationTurnPhase,
) {
  if (!conversationId) return;
  phases[conversationId] = phase;
}
