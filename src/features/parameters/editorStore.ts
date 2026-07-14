import { SIDEBAR_FOOTER_STATUS } from "@lilia/ui";
import { computed, reactive } from "vue";
import {
  cancelParameterBatch,
  connectEditor,
  disconnectEditor,
  executeParameterBatch,
  getEditorSnapshot,
  listenBatchFinished,
  listenBatchProgress,
  listenEditorState,
  normalizeCommandError,
  previewParameterBatch,
} from "./bridge";
import type {
  BatchFinished,
  BatchProgress,
  DomainCommandError,
  EditorSnapshot,
  ParameterBatchInput,
  ParameterBatchPreview,
} from "./types";

const defaultSnapshot: EditorSnapshot = {
  state: "disconnected",
  port: 22033,
  apiVersion: null,
  modelLabel: null,
  groups: [],
  capabilities: { batchCreateParameters: false },
  message: "尚未连接 Cubism Editor。",
};

const state = reactive({
  snapshot: defaultSnapshot,
  activeOperationId: null as string | null,
  progress: null as BatchProgress | null,
  finished: null as BatchFinished | null,
  error: null as DomainCommandError | null,
  initialized: false,
  busy: false,
});

let initializePromise: Promise<void> | null = null;

export function useEditorStore() {
  const canCreate = computed(
    () => state.snapshot.state === "ready" && state.snapshot.capabilities.batchCreateParameters,
  );
  const operationActive = computed(() => Boolean(state.activeOperationId));

  async function initialize() {
    if (initializePromise) return initializePromise;
    initializePromise = (async () => {
      await Promise.all([
        listenEditorState((snapshot) => updateSnapshot(snapshot)),
        listenBatchProgress((progress) => {
          if (!state.activeOperationId || progress.operationId === state.activeOperationId) {
            state.activeOperationId = progress.operationId;
            state.progress = progress;
          }
        }),
        listenBatchFinished((finished) => {
          if (!state.activeOperationId || finished.operationId === state.activeOperationId) {
            state.finished = finished;
            state.activeOperationId = null;
            state.progress = null;
          }
        }),
      ]);
      updateSnapshot(await getEditorSnapshot());
      state.initialized = true;
    })().catch((error) => {
      state.error = normalizeCommandError(error);
      state.initialized = true;
    });
    return initializePromise;
  }

  async function connect(port: number) {
    state.busy = true;
    state.error = null;
    state.finished = null;
    try {
      updateSnapshot(await connectEditor(port));
    } catch (error) {
      state.error = normalizeCommandError(error);
    } finally {
      state.busy = false;
    }
  }

  async function disconnect() {
    state.busy = true;
    state.error = null;
    try {
      await disconnectEditor();
      updateSnapshot(await getEditorSnapshot());
    } catch (error) {
      state.error = normalizeCommandError(error);
    } finally {
      state.busy = false;
    }
  }

  async function preview(input: ParameterBatchInput): Promise<ParameterBatchPreview | null> {
    state.busy = true;
    state.error = null;
    state.finished = null;
    try {
      return await previewParameterBatch(input);
    } catch (error) {
      state.error = normalizeCommandError(error);
      return null;
    } finally {
      state.busy = false;
    }
  }

  async function execute(previewId: string) {
    state.busy = true;
    state.error = null;
    state.finished = null;
    try {
      const accepted = await executeParameterBatch(previewId);
      const finished = state.finished as BatchFinished | null;
      if (finished?.operationId !== accepted.operationId) {
        state.activeOperationId = accepted.operationId;
      }
    } catch (error) {
      state.error = normalizeCommandError(error);
    } finally {
      state.busy = false;
    }
  }

  async function cancel() {
    if (!state.activeOperationId) return;
    state.error = null;
    try {
      await cancelParameterBatch(state.activeOperationId);
    } catch (error) {
      state.error = normalizeCommandError(error);
    }
  }

  function clearMessages() {
    state.error = null;
    state.finished = null;
  }

  return {
    state,
    canCreate,
    operationActive,
    initialize,
    connect,
    disconnect,
    preview,
    execute,
    cancel,
    clearMessages,
  };
}

function updateSnapshot(snapshot: EditorSnapshot) {
  state.snapshot = snapshot;
  const presentation = footerPresentation(snapshot);
  Object.assign(SIDEBAR_FOOTER_STATUS, {
    to: "/",
    label: presentation.label,
    title: snapshot.message,
    tone: presentation.tone,
  });
}

function footerPresentation(snapshot: EditorSnapshot): {
  label: string;
  tone: "ok" | "warn" | "error";
} {
  switch (snapshot.state) {
    case "ready":
      return { label: "Editor 已就绪", tone: "ok" };
    case "editing":
      return { label: "正在创建", tone: "warn" };
    case "cancelling":
      return { label: "正在取消", tone: "warn" };
    case "connecting":
      return { label: "正在连接", tone: "warn" };
    case "awaiting_access":
    case "awaiting_edit_permission":
      return { label: "等待授权", tone: "warn" };
    case "failed":
      return { label: "连接异常", tone: "error" };
    case "incompatible":
      return { label: "当前不可用", tone: "warn" };
    default:
      return { label: "未连接", tone: "warn" };
  }
}
