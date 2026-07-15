import type { EditorConnectionState } from "../editor/types";
import type { ChatMessage, LlmConfigView } from "./types";

export function editorStatusLabel(state: EditorConnectionState) {
  switch (state) {
    case "ready":
      return "Editor 已就绪";
    case "editing":
      return "正在编辑";
    case "cancelling":
      return "正在取消编辑";
    case "connecting":
      return "正在连接 Editor";
    case "awaiting_access":
    case "awaiting_edit_permission":
      return "等待 Editor 授权";
    case "failed":
      return "Editor 连接异常";
    case "incompatible":
      return "Editor 当前不可用";
    default:
      return "Editor 未连接";
  }
}

export function modelStatusLabel(config: LlmConfigView) {
  if (!config.hasApiKey) return "模型未配置";
  return config.model || "模型已配置";
}

const TOOL_LABELS: Record<string, string> = {
  get_editor_snapshot: "检查 Editor 状态",
  connect_editor: "连接 Cubism Editor",
  disconnect_editor: "断开 Editor 连接",
  find_selected_part_parameters: "读取选中 Part 参数",
  preview_parameter_batch: "预览参数修改",
  execute_parameter_batch: "应用参数修改",
  cancel_parameter_batch: "取消参数修改",
  capture_cubism_editor_window: "查看 Editor 窗口",
  list_projects: "读取项目",
  bind_conversation_project: "更新项目归属",
  list_memories: "读取记忆",
  upsert_memory: "更新记忆",
  archive_memory: "停用记忆",
  ask_user: "等待确认",
  update_plan: "更新计划",
};

export interface ToolActivityPresentation {
  label: string;
  detail: string | null;
  status: "started" | "finished" | "failed" | "unknown";
}

export function toolActivityPresentation(message: ChatMessage): ToolActivityPresentation {
  const status = message.toolStatus === "started"
    || message.toolStatus === "finished"
    || message.toolStatus === "failed"
    ? message.toolStatus
    : "unknown";
  return {
    label: TOOL_LABELS[message.toolName ?? ""] ?? "执行操作",
    detail: status === "failed" ? message.content : null,
    status,
  };
}
