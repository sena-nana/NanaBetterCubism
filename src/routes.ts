import { LiliaSettingsPage } from "@lilia/ui";
import type { RouteRecordRaw } from "vue-router";

const HomePage = () => import("./features/home/HomePage.vue");

export const routes: RouteRecordRaw[] = [
  { path: "", component: HomePage, meta: { sidebar: "main", returnable: true } },
  {
    path: "settings",
    component: LiliaSettingsPage,
    meta: { sidebar: "settings", lockSidebar: true, returnable: false },
  },
];
