import type { EditorConnectionState } from "../editor/types";
import type { LlmConfigView } from "./types";

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
