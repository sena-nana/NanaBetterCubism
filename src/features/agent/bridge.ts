import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { domainError, isTauriRuntime, normalizeCommandError } from "../editor/bridge";
import type {
  AgentComputerOperationEvent,
  AgentPlanEvent,
  AgentToolEvent,
  AgentTurnDelta,
  AgentTurnFinished,
  AgentUserActionEvent,
  CancelTurnResult,
  ChatMessage,
  ConversationPlan,
  ConversationSummary,
  LlmConfigInput,
  LlmConfigView,
  LlmTestResult,
  MemoryRecord,
  PendingUserAction,
  ProjectRecord,
} from "./types";

export { normalizeCommandError };

export async function listConversations(): Promise<ConversationSummary[]> {
  if (!isTauriRuntime()) return [];
  return invoke<ConversationSummary[]>("agent_list_conversations");
}

export async function createConversation(title?: string): Promise<ConversationSummary> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中创建对话。");
  }
  return invoke<ConversationSummary>("agent_create_conversation", { title: title ?? null });
}

export async function getMessages(conversationId: string): Promise<ChatMessage[]> {
  if (!isTauriRuntime()) return [];
  return invoke<ChatMessage[]>("agent_get_messages", { conversationId });
}

export async function sendMessage(conversationId: string, content: string): Promise<void> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中发送消息。");
  }
  await invoke("agent_send_message", { conversationId, content });
}

export async function cancelTurn(conversationId: string): Promise<CancelTurnResult> {
  if (!isTauriRuntime()) return { state: "idle" };
  return invoke<CancelTurnResult>("agent_cancel_turn", { conversationId });
}

export async function setConversationPinned(
  conversationId: string,
  pinned: boolean,
): Promise<boolean> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中管理对话。");
  }
  return invoke<boolean>("agent_set_conversation_pinned", { conversationId, pinned });
}

export async function deleteConversation(conversationId: string): Promise<void> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中管理对话。");
  }
  await invoke("agent_delete_conversation", { conversationId });
}

export async function answerQuestion(actionId: string, answer: string): Promise<void> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中回答提问。");
  }
  await invoke("agent_answer_question", { actionId, answer });
}

export async function decideComputerOperation(
  actionId: string,
  approved: boolean,
): Promise<void> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中处理电脑代理授权。");
  }
  await invoke("agent_decide_computer_operation", { actionId, approved });
}

export async function getPlan(conversationId: string): Promise<ConversationPlan | null> {
  if (!isTauriRuntime()) return null;
  return invoke<ConversationPlan | null>("agent_get_plan", { conversationId });
}

export async function getPendingUserAction(
  conversationId: string,
): Promise<PendingUserAction | null> {
  if (!isTauriRuntime()) return null;
  return invoke<PendingUserAction | null>("agent_get_pending_user_action", { conversationId });
}

export async function listProjects(): Promise<ProjectRecord[]> {
  if (!isTauriRuntime()) return [];
  return invoke<ProjectRecord[]>("agent_list_projects");
}

export async function listMemories(projectId?: string | null): Promise<MemoryRecord[]> {
  if (!isTauriRuntime()) return [];
  return invoke<MemoryRecord[]>("memory_list", { projectId: projectId ?? null });
}

export async function setMemoryEnabled(id: string, enabled: boolean): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("memory_set_enabled", { id, enabled });
}

export async function getLlmConfig(): Promise<LlmConfigView> {
  if (!isTauriRuntime()) {
    return { baseUrl: null, model: null, hasApiKey: false };
  }
  return invoke<LlmConfigView>("llm_get_config");
}

export async function setLlmConfig(input: LlmConfigInput): Promise<LlmConfigView> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中配置模型。");
  }
  return invoke<LlmConfigView>("llm_set_config", { input });
}

export async function testLlmConnection(): Promise<LlmTestResult> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请在桌面应用中测试连接。");
  }
  return invoke<LlmTestResult>("llm_test_connection");
}

export async function listenConversationsChanged(handler: () => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen("agent://conversations-changed", () => handler());
}

export async function listenTurnDelta(handler: (payload: AgentTurnDelta) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentTurnDelta>("agent://turn-delta", (e) => handler(e.payload));
}

export async function listenToolEvent(handler: (payload: AgentToolEvent) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentToolEvent>("agent://tool", (e) => handler(e.payload));
}

export async function listenTurnFinished(handler: (payload: AgentTurnFinished) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentTurnFinished>("agent://turn-finished", (e) => handler(e.payload));
}

export async function listenUserAction(handler: (payload: AgentUserActionEvent) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentUserActionEvent>("agent://user-action", (e) => handler(e.payload));
}

export async function listenComputerOperation(
  handler: (payload: AgentComputerOperationEvent) => void,
) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentComputerOperationEvent>("agent://computer-operation", (e) =>
    handler(e.payload),
  );
}

export async function listenPlan(handler: (payload: AgentPlanEvent) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<AgentPlanEvent>("agent://plan", (e) => handler(e.payload));
}

const noopUnlisten: UnlistenFn = () => {};
