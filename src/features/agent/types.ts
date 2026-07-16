export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface ConversationSummary {
  id: string;
  title: string;
  projectId: string | null;
  projectName: string | null;
  updatedAt: string;
  pinned: boolean;
}

export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  toolName: string | null;
  toolDisplayName: string | null;
  toolStatus: string | null;
  createdAt: string;
}

export interface PlanStep {
  id: string;
  title: string;
  status: "pending" | "in_progress" | "completed" | "cancelled";
}

export interface ConversationPlan {
  conversationId: string;
  steps: PlanStep[];
  updatedAt: string;
}

export interface ProjectRecord {
  id: string;
  name: string;
  updatedAt: string;
}

export type MemoryScope = "project" | "global";
export type MemoryKind = "stage" | "experience";

export interface MemoryRecord {
  id: string;
  scope: MemoryScope;
  kind: MemoryKind;
  projectId: string | null;
  projectName: string | null;
  title: string;
  body: string;
  enabled: boolean;
  sourceConversationId: string | null;
  updatedAt: string;
}

export interface LlmConfigView {
  baseUrl: string | null;
  model: string | null;
  hasApiKey: boolean;
}

export interface LlmConfigInput {
  baseUrl: string | null;
  apiKey: string | null;
  model: string | null;
  clearApiKey?: boolean;
}

export interface LlmTestResult {
  ok: boolean;
  message: string;
  models: string[];
}

export interface AgentTurnDelta {
  conversationId: string;
  text: string;
}

export interface AgentToolEvent {
  conversationId: string;
  toolCallId: string;
  toolName: string;
  toolDisplayName: string;
  status: "started" | "finished" | "failed";
  summary: string;
}

export interface AgentTurnFinished {
  conversationId: string;
  ok: boolean;
  message: string;
}

export type ComputerActionKind =
  | "click"
  | "double_click"
  | "drag"
  | "scroll"
  | "key"
  | "type_text";

export interface PendingQuestionAction {
  kind: "question";
  actionId: string;
  conversationId: string;
  question: string;
  options: string[];
}

export interface ComputerApprovalAction {
  kind: "computer_approval";
  actionId: string;
  conversationId: string;
  goal: string;
  reason: string;
  targetWindowTitle: string;
  steps: Array<{ id: string; title: string }>;
  allowedActions: ComputerActionKind[];
  includesFileDialogs: boolean;
  impact: string;
  cannotUndo: boolean;
  expiresAt: string;
}

export type PendingUserAction = PendingQuestionAction | ComputerApprovalAction;

export type ComputerOperationStatus =
  | "idle"
  | "awaiting_approval"
  | "authorized"
  | "running"
  | "completed"
  | "needs_user_verification"
  | "cancelled"
  | "failed"
  | "unknown";

export interface CancelTurnResult {
  state: "cancel_requested" | "pending_cleared" | "idle";
}

export interface AgentUserActionEvent {
  conversationId: string;
  action: PendingUserAction;
}

export interface AgentComputerOperationEvent {
  conversationId: string;
  status: ComputerOperationStatus;
}

export interface AgentPlanEvent {
  conversationId: string;
  plan: ConversationPlan;
}
