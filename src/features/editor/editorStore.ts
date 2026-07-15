import { SIDEBAR_FOOTER_STATUS } from "@lilia/ui";
import { reactive } from "vue";
import {
  connectEditor,
  disconnectEditor,
  getEditorSnapshot,
  listenEditorState,
  normalizeCommandError,
} from "./bridge";
import type { DomainCommandError, EditorSnapshot } from "./types";

const defaultSnapshot: EditorSnapshot = {
  state: "disconnected",
  port: 22033,
  apiVersion: null,
  modelLabel: null,
  groups: [],
  capabilities: {
    batchCreateParameters: false,
    findPartParameters: false,
    officialApi: false,
    officialEditApi: false,
  },
  message: "尚未连接 Cubism Editor。",
};

const state = reactive({
  snapshot: defaultSnapshot,
  error: null as DomainCommandError | null,
  initialized: false,
  busy: false,
});

let initializePromise: Promise<void> | null = null;

export function useEditorStore() {
  async function initialize() {
    if (initializePromise) return initializePromise;
    initializePromise = (async () => {
      await listenEditorState((snapshot) => updateSnapshot(snapshot));
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

  return { state, initialize, connect, disconnect };
}

function updateSnapshot(snapshot: EditorSnapshot) {
  state.snapshot = snapshot;
  const presentation = footerPresentation(snapshot);
  Object.assign(SIDEBAR_FOOTER_STATUS, {
    to: "/settings?tab=editor",
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
      return { label: "正在编辑", tone: "warn" };
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
