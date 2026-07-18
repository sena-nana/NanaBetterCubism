import { LiliaSettingsPage } from "./ui";
import type { RouteRecordRaw } from "vue-router";

const HomePage = () => import("./features/home/HomePage.vue");
const ChatPage = () => import("./features/agent/ChatPage.vue");
const MemoryPage = () => import("./features/agent/MemoryPage.vue");

export const routes: RouteRecordRaw[] = [
  { path: "/", component: HomePage },
  {
    path: "/chats/:id",
    component: ChatPage,
  },
  {
    path: "/memory",
    component: MemoryPage,
  },
  {
    path: "/settings",
    component: LiliaSettingsPage,
    meta: { sidebar: "settings", returnable: false },
  },
];
