import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BatchFinished,
  BatchProgress,
  DomainCommandError,
  EditorSnapshot,
  OperationAccepted,
  ParameterBatchInput,
  ParameterBatchPreview,
} from "./types";

const fallbackSnapshot: EditorSnapshot = {
  state: "disconnected",
  port: 22033,
  apiVersion: null,
  modelLabel: null,
  groups: [],
  capabilities: { batchCreateParameters: false, findPartParameters: false },
  message: "请在桌面应用中连接 Cubism Editor。",
};

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getEditorSnapshot(): Promise<EditorSnapshot> {
  if (!isTauriRuntime()) return fallbackSnapshot;
  return invoke<EditorSnapshot>("get_editor_snapshot");
}

export async function connectEditor(port: number): Promise<EditorSnapshot> {
  if (!isTauriRuntime()) throw domainError("desktop_required", "请在桌面应用中使用 Editor 连接。");
  return invoke<EditorSnapshot>("connect_editor", { port });
}

export async function disconnectEditor(): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("disconnect_editor");
}

export async function previewParameterBatch(
  input: ParameterBatchInput,
): Promise<ParameterBatchPreview> {
  if (!isTauriRuntime()) throw domainError("desktop_required", "请先在桌面应用中连接 Editor。");
  return invoke<ParameterBatchPreview>("preview_parameter_batch", { input });
}

export async function executeParameterBatch(previewId: string): Promise<OperationAccepted> {
  if (!isTauriRuntime()) throw domainError("desktop_required", "请先在桌面应用中连接 Editor。");
  return invoke<OperationAccepted>("execute_parameter_batch", { previewId });
}

export async function cancelParameterBatch(operationId: string): Promise<void> {
  if (!isTauriRuntime()) return;
  await invoke("cancel_parameter_batch", { operationId });
}

export async function listenEditorState(handler: (snapshot: EditorSnapshot) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<EditorSnapshot>("cubism://editor-state", (event) => handler(event.payload));
}

export async function listenBatchProgress(handler: (progress: BatchProgress) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<BatchProgress>("cubism://parameter-batch-progress", (event) => handler(event.payload));
}

export async function listenBatchFinished(handler: (finished: BatchFinished) => void) {
  if (!isTauriRuntime()) return noopUnlisten;
  return listen<BatchFinished>("cubism://parameter-batch-finished", (event) => handler(event.payload));
}

export function normalizeCommandError(error: unknown): DomainCommandError {
  if (typeof error === "object" && error !== null) {
    const value = error as Partial<DomainCommandError>;
    if (typeof value.code === "string" && typeof value.message === "string") {
      return { code: value.code, message: value.message };
    }
  }
  return domainError("unexpected_error", error instanceof Error ? error.message : String(error));
}

export function domainError(code: string, message: string): DomainCommandError {
  return { code, message };
}

const noopUnlisten: UnlistenFn = () => {};
