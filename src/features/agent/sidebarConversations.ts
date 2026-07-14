import { setLiliaAppConfig } from "@lilia/ui";
import type { Router } from "vue-router";
import { appConfig } from "../../app.config";
import {
  createConversation,
  listConversations,
  listenConversationsChanged,
} from "./bridge";
import type { ConversationSummary } from "./types";

let loadPromise: Promise<ConversationSummary[]> | null = null;
let newChatHandler: (() => void | Promise<void>) | undefined;

export async function ensureSidebarConversationsLoaded(force = false) {
  if (loadPromise && !force) return loadPromise;
  loadPromise = (async () => {
    const rows = await listConversations();
    applyConversationGroup(rows);
    return rows;
  })();
  return loadPromise;
}

export function applyConversationGroup(rows: ConversationSummary[]) {
  setLiliaAppConfig({
    ...appConfig,
    sidebar: {
      ...appConfig.sidebar,
      globalActions: [
        {
          key: "new-chat",
          label: "新对话",
          icon: "file-plus",
          onSelect: newChatHandler,
        },
      ],
      groups: [
        {
          key: "conversations",
          title: "对话",
          emptyText: rows.length === 0 ? "暂无对话" : undefined,
          items: rows.map((row) => ({
            key: row.id,
            label: row.title,
            icon: "bot",
            to: `/chats/${row.id}`,
          })),
        },
      ],
    },
  });
}

export function installAgentShell(router: Router) {
  newChatHandler = async () => {
    try {
      const created = await createConversation();
      await ensureSidebarConversationsLoaded(true);
      await router.push(`/chats/${created.id}`);
    } catch {
      await router.push("/");
    }
  };

  setLiliaAppConfig({
    ...appConfig,
    sidebar: {
      ...appConfig.sidebar,
      globalActions: [
        {
          key: "new-chat",
          label: "新对话",
          icon: "file-plus",
          onSelect: newChatHandler,
        },
      ],
    },
  });

  void ensureSidebarConversationsLoaded();
  void listenConversationsChanged(() => {
    void ensureSidebarConversationsLoaded(true);
  });
}
