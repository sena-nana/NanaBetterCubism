import { LiliaSettingsPage } from "@lilia/ui";
import type { RouteRecordRaw } from "vue-router";

const ChatHomePage = () => import("./features/agent/ChatHomePage.vue");
const ChatPage = () => import("./features/agent/ChatPage.vue");
const MemoryPage = () => import("./features/agent/MemoryPage.vue");

export const routes: RouteRecordRaw[] = [
  { path: "", component: ChatHomePage, meta: { sidebar: "main", returnable: true } },
  {
    path: "chats/:id",
    component: ChatPage,
    meta: { sidebar: "main", returnable: true },
  },
  {
    path: "memory",
    component: MemoryPage,
    meta: { sidebar: "main", returnable: true },
  },
  {
    path: "settings",
    component: LiliaSettingsPage,
    meta: { sidebar: "settings", lockSidebar: true, returnable: false },
  },
];
