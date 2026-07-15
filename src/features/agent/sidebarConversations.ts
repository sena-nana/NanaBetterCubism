import FilePlus from "@lucide/vue/dist/esm/icons/file-plus.mjs";
import MessageSquare from "@lucide/vue/dist/esm/icons/message-square.mjs";
import RotateCcw from "@lucide/vue/dist/esm/icons/rotate-ccw.mjs";
import {
  SIDEBAR_GLOBAL_ACTIONS,
  SIDEBAR_GROUPS,
  type SidebarGroup,
  type SidebarNavItem,
} from "@lilia/ui";
import { markRaw } from "vue";
import type { Router } from "vue-router";
import { listConversations, listenConversationsChanged } from "./bridge";
import type { ConversationSummary } from "./types";

const conversationIcon = markRaw(MessageSquare);
const newChatIcon = markRaw(FilePlus);
const retryIcon = markRaw(RotateCcw);

let loadPromise: Promise<ConversationSummary[]> | null = null;
let loadEpoch = 0;
let loadedRows: ConversationSummary[] | null = null;

export function ensureSidebarConversationsLoaded(force = false) {
  if (loadPromise && !force) {
    if (loadedRows !== null) applyConversationGroup(loadedRows);
    return loadPromise;
  }

  const epoch = ++loadEpoch;
  if (loadedRows === null) applyLoadingGroup();

  const request = listConversations()
    .then((rows) => {
      if (epoch === loadEpoch) {
        loadedRows = rows;
        applyConversationGroup(rows);
      }
      return rows;
    })
    .catch((error: unknown) => {
      if (epoch === loadEpoch) applyErrorGroup();
      if (loadPromise === request) loadPromise = null;
      throw error;
    });

  loadPromise = request;
  return request;
}

export function applyConversationGroup(rows: ConversationSummary[]) {
  const grouped = new Map<string, SidebarNavItem[]>();

  for (const row of rows) {
    const projectName = row.projectName?.trim() || "收集箱";
    if (!grouped.has(projectName)) grouped.set(projectName, []);
    const items = grouped.get(projectName)!;
    items.push({
      key: row.id,
      label: row.title,
      icon: conversationIcon,
      to: `/chats/${row.id}`,
    });
  }

  const groups: SidebarGroup[] = [...grouped].map(([title, items]) => ({
    key: title === "收集箱" ? "inbox" : `project:${title}`,
    title,
    items,
  }));

  if (groups.length === 0) {
    groups.push({ key: "inbox", title: "收集箱", emptyText: "暂无对话", items: [] });
  }

  replaceGroups(groups);
}

export function installAgentShell(router: Router) {
  SIDEBAR_GLOBAL_ACTIONS.splice(0, SIDEBAR_GLOBAL_ACTIONS.length, {
    key: "new-chat",
    label: "新对话",
    icon: newChatIcon,
    onSelect: () => router.push("/").then(() => undefined),
  });

  void ensureSidebarConversationsLoaded().catch(() => undefined);
  void listenConversationsChanged(() => {
    void ensureSidebarConversationsLoaded(true).catch(() => undefined);
  }).catch(() => undefined);
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
          onSelect: () =>
            ensureSidebarConversationsLoaded(true)
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
