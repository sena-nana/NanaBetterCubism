import {
  SIDEBAR_FOOTER_STATUS,
  SIDEBAR_FOOTER_STATUSES,
  type SidebarFooterStatus,
} from "@lilia/ui";
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
    if (state.initialized) {
      if (state.error) updateEditorErrorFooter();
      else updateSnapshot(state.snapshot);
      return;
    }
    if (initializePromise) return initializePromise;
    initializePromise = (async () => {
      await listenEditorState((snapshot) => updateSnapshot(snapshot));
      updateSnapshot(await getEditorSnapshot());
      state.initialized = true;
    })().catch((error) => {
      state.error = normalizeCommandError(error);
      state.initialized = true;
      updateEditorErrorFooter();
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
      updateEditorErrorFooter();
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
      updateEditorErrorFooter();
    } finally {
      state.busy = false;
    }
  }

  return { state, initialize, connect, disconnect };
}

function updateSnapshot(snapshot: EditorSnapshot) {
  state.snapshot = snapshot;
  const presentation = footerPresentation(snapshot);
  updateEditorFooter({
    to: "/settings?tab=editor",
    label: presentation.label,
    title: snapshot.message,
    tone: presentation.tone,
  });
}

function updateEditorFooter(presentation: {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
  to?: string;
}) {
  const footer = footerStatus("editor");
  if (!footer) return;
  Object.assign(footer, { to: "/settings?tab=editor", ...presentation });
}

function updateEditorErrorFooter() {
  updateEditorFooter({
    label: "Editor 状态异常",
    title: "无法读取 Editor 状态。点击进入设置。",
    tone: "error",
  });
}

function footerStatus(key: string): SidebarFooterStatus | undefined {
  return SIDEBAR_FOOTER_STATUSES.find((status) => status.key === key)
    ?? (SIDEBAR_FOOTER_STATUSES.length === 1 ? SIDEBAR_FOOTER_STATUS : undefined);
}

function footerPresentation(snapshot: EditorSnapshot): {
  label: string;
  tone: "ok" | "warn" | "error";
} {
  switch (snapshot.state) {
    case "ready":
      return { label: "Editor 已就绪", tone: "ok" };
    case "editing":
      return { label: "Editor 编辑中", tone: "warn" };
    case "cancelling":
      return { label: "Editor 取消中", tone: "warn" };
    case "connecting":
      return { label: "Editor 连接中", tone: "warn" };
    case "awaiting_access":
    case "awaiting_edit_permission":
      return { label: "Editor 等待授权", tone: "warn" };
    case "failed":
      return { label: "Editor 连接异常", tone: "error" };
    case "incompatible":
      return { label: "Editor 当前不可用", tone: "warn" };
    default:
      return { label: "Editor 未连接", tone: "warn" };
  }
}
