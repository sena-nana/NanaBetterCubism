import Archive from "@lucide/vue/dist/esm/icons/archive.mjs";
import MessageSquare from "@lucide/vue/dist/esm/icons/message-square.mjs";
import Pin from "@lucide/vue/dist/esm/icons/pin.mjs";
import RotateCcw from "@lucide/vue/dist/esm/icons/rotate-ccw.mjs";
import {
  SIDEBAR_GROUPS,
  type SidebarGroup,
  type SidebarNavItem,
} from "@lilia/ui";
import { markRaw, reactive } from "vue";
import {
  archiveConversation,
  listConversations,
  listenConversationsChanged,
  normalizeCommandError,
  setConversationPinned,
} from "./bridge";
import {
  getConversationTurnPhase,
  subscribeConversationTurnPhases,
} from "./conversationRuntimeStore";
import type { ConversationSummary } from "./types";

const conversationIcon = markRaw(MessageSquare);
const pinIcon = markRaw(Pin);
const archiveIcon = markRaw(Archive);
const retryIcon = markRaw(RotateCcw);

export const sidebarConversationsState = reactive({
  rows: [] as ConversationSummary[],
  loaded: false,
  loading: false,
  loadError: null as string | null,
  actionError: null as string | null,
  archiveTarget: null as ConversationSummary | null,
  archiving: false,
});

let loadPromise: Promise<ConversationSummary[]> | null = null;
let loadEpoch = 0;
let listenerInstalled = false;
let unsubscribePhases: (() => void) | null = null;

export function ensureSidebarConversationsLoaded(force = false) {
  if (loadPromise && !force) return loadPromise;
  if (sidebarConversationsState.loaded && !force) {
    applyConversationGroup(sidebarConversationsState.rows);
    return Promise.resolve(sidebarConversationsState.rows);
  }

  const epoch = ++loadEpoch;
  sidebarConversationsState.loading = true;
  sidebarConversationsState.loadError = null;
  if (!sidebarConversationsState.loaded) applyLoadingGroup();

  const request = listConversations()
    .then((rows) => {
      if (epoch === loadEpoch) applyConversationGroup(rows);
      return rows;
    })
    .catch((error: unknown) => {
      if (epoch === loadEpoch) {
        sidebarConversationsState.loadError = normalizeCommandError(error).message;
        if (!sidebarConversationsState.loaded) applyErrorGroup();
      }
      throw error;
    })
    .finally(() => {
      if (epoch === loadEpoch) sidebarConversationsState.loading = false;
      if (loadPromise === request) loadPromise = null;
    });

  loadPromise = request;
  return request;
}

export function applyConversationGroup(rows: ConversationSummary[]) {
  sidebarConversationsState.rows = rows;
  sidebarConversationsState.loaded = true;
  sidebarConversationsState.loadError = null;

  const grouped = new Map<string, { title: string; items: SidebarNavItem[] }>();
  for (const row of rows) {
    const key = row.projectId ? `project:${row.projectId}` : "inbox";
    const title = row.projectName?.trim() || "收集箱";
    const group = grouped.get(key) ?? { title, items: [] };
    const phase = getConversationTurnPhase(row.id);
    group.items.push({
      key: row.id,
      label: row.title,
      icon: conversationIcon,
      to: `/chats/${row.id}`,
      badges: phase === "idle"
        ? undefined
        : [{
            key: "phase",
            label: phaseLabel(phase),
            tone: phase === "awaiting_input" ? "warn" : "muted",
          }],
      tools: [
        {
          key: "pin",
          label: row.pinned ? "取消置顶" : "置顶",
          icon: pinIcon,
          active: row.pinned,
          onSelect: () => toggleConversationPinned(row),
        },
        {
          key: "archive",
          label: phase === "idle" ? "归档" : "对话进行中，无法归档",
          icon: archiveIcon,
          disabled: phase !== "idle",
          onSelect: () => requestConversationArchive(row),
        },
      ],
    });
    grouped.set(key, group);
  }

  const groups: SidebarGroup[] = [...grouped].map(([key, group]) => ({
    key,
    title: group.title,
    items: group.items,
  }));
  if (groups.length === 0) {
    groups.push({ key: "inbox", title: "收集箱", emptyText: "暂无对话", items: [] });
  }
  replaceGroups(groups);
}

export function installAgentShell() {
  void ensureSidebarConversationsLoaded().catch(() => undefined);
  if (!listenerInstalled) {
    listenerInstalled = true;
    void listenConversationsChanged(() => {
      void ensureSidebarConversationsLoaded(true).catch(() => undefined);
    }).catch(() => {
      listenerInstalled = false;
    });
  }
  unsubscribePhases?.();
  unsubscribePhases = subscribeConversationTurnPhases(() => {
    if (sidebarConversationsState.loaded) {
      applyConversationGroup(sidebarConversationsState.rows);
    }
  });
}

export async function toggleConversationPinned(row: ConversationSummary) {
  sidebarConversationsState.actionError = null;
  try {
    const pinned = await setConversationPinned(row.id, !row.pinned);
    const rows = sidebarConversationsState.rows
      .map((item) => item.id === row.id ? { ...item, pinned } : item)
      .sort(compareConversations);
    applyConversationGroup(rows);
  } catch (error) {
    sidebarConversationsState.actionError = normalizeCommandError(error).message;
  }
}

export function requestConversationArchive(row: ConversationSummary) {
  sidebarConversationsState.actionError = null;
  sidebarConversationsState.archiveTarget = row;
}

export function cancelConversationArchive() {
  if (sidebarConversationsState.archiving) return;
  sidebarConversationsState.archiveTarget = null;
}

export async function confirmConversationArchive(): Promise<string | null> {
  const target = sidebarConversationsState.archiveTarget;
  if (!target || sidebarConversationsState.archiving) return null;
  sidebarConversationsState.archiving = true;
  sidebarConversationsState.actionError = null;
  try {
    await archiveConversation(target.id);
    sidebarConversationsState.archiveTarget = null;
    applyConversationGroup(
      sidebarConversationsState.rows.filter((row) => row.id !== target.id),
    );
    return target.id;
  } catch (error) {
    sidebarConversationsState.actionError = normalizeCommandError(error).message;
    return null;
  } finally {
    sidebarConversationsState.archiving = false;
  }
}

export function dismissConversationError() {
  sidebarConversationsState.actionError = null;
  sidebarConversationsState.loadError = null;
}

function compareConversations(left: ConversationSummary, right: ConversationSummary) {
  return Number(right.pinned) - Number(left.pinned)
    || right.updatedAt.localeCompare(left.updatedAt);
}

function phaseLabel(phase: ReturnType<typeof getConversationTurnPhase>) {
  if (phase === "running") return "运行中";
  if (phase === "awaiting_input") return "等待回答";
  if (phase === "cancelling") return "取消中";
  return "";
}

function applyLoadingGroup() {
  replaceGroups([
    { key: "conversations-loading", title: "对话", emptyText: "正在加载对话…", items: [] },
  ]);
}

function applyErrorGroup() {
  replaceGroups([
    {
      key: "conversations-error",
      title: "对话",
      emptyText: "无法加载对话",
      items: [],
      tools: [
        {
          key: "retry",
          label: "重试",
          icon: retryIcon,
          onSelect: () => ensureSidebarConversationsLoaded(true)
            .then(() => undefined)
            .catch(() => undefined),
        },
      ],
    },
  ]);
}

function replaceGroups(groups: SidebarGroup[]) {
  SIDEBAR_GROUPS.splice(0, SIDEBAR_GROUPS.length, ...groups);
}
