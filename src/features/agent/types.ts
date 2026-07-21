export type MessageRole = "user" | "assistant" | "tool" | "system";
export type AgentTurnMode = "default" | "conversation_only" | "plan" | "auto_approve";

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
  attachments: ChatImageAttachment[];
  createdAt: string;
}

export interface ChatImageAttachment {
  id: string;
  name: string;
  path: string;
  mime: string;
  size: number;
  available: boolean;
}

export interface ChatImageDraft extends ChatImageAttachment {
  draftId: string;
}

export type ImagePrepareInput =
  | { kind: "path"; path: string }
  | { kind: "bytes"; name?: string; bytesBase64: string };

export interface ImagePrepareRejection {
  index: number;
  name: string;
  code: string;
  message: string;
}

export interface ImagePrepareResult {
  accepted: ChatImageDraft[];
  rejected: ImagePrepareRejection[];
}

export interface ChatPsdDocument {
  id: string;
  name: string;
  path: string;
  width: number;
  height: number;
  colorMode: string;
  layerCount: number;
  available: boolean;
}

export interface ChatPsdDraft {
  id: string;
  name: string;
  path: string;
}

export interface PsdBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PsdMaskInfo {
  present: boolean;
  disabled: boolean;
  invert: boolean;
  defaultColor: number;
  bounds: PsdBounds | null;
}

export interface PsdLayerNode {
  id: string;
  name: string;
  kind: "group" | "group_end" | "layer";
  visible: boolean;
  opacity: number;
  blendMode: string;
  isClipped: boolean;
  hasMask: boolean;
  mask: PsdMaskInfo;
  bounds: PsdBounds;
  children: PsdLayerNode[];
}

export interface PsdStructure {
  width: number;
  height: number;
  colorMode: string;
  depth: number;
  channelCount: number;
  layerCount: number;
  layers: PsdLayerNode[];
}

export interface PsdPrepareResult {
  document: ChatPsdDocument;
  structure: PsdStructure;
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

export interface MemoryLayer {
  name: string;
  content: string;
}

export interface MemoryRecord {
  id: string;
  scope: MemoryScope;
  projectId: string | null;
  projectName: string | null;
  title: string;
  layers: MemoryLayer[];
  enabled: boolean;
  sourceConversationId: string | null;
  updatedAt: string;
  revision: number;
}

export interface LlmConfigView {
  baseUrl: string | null;
  model: string | null;
  hasApiKey: boolean;
  imageInputSupported?: boolean | null;
  contextWindow?: number | null;
  maxInputTokens?: number | null;
}

export interface LlmConfigInput {
  baseUrl: string | null;
  apiKey: string | null;
  model: string | null;
  clearApiKey?: boolean;
  contextWindow?: number | null;
  maxInputTokens?: number | null;
}

export interface LlmTestResult {
  ok: boolean;
  message: string;
  models: string[];
  imageSupported?: boolean | null;
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

export interface PlanApprovalAction {
  kind: "plan_approval";
  actionId: string;
  conversationId: string;
  title: string;
}

export type PendingUserAction = PendingQuestionAction | PlanApprovalAction;
export type PlanDecision = "approve" | "revise" | "cancel";
export type PlanDecisionResult = "execution_started" | "revision_started" | "cancelled";

export type ComputerOperationStatus =
  | "idle"
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

export interface AgentImageCapabilityEvent {
  supported: boolean;
  unsupported: boolean;
  reason?: string | null;
}
