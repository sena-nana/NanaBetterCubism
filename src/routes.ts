import { LiliaSettingsPage } from "@lilia/ui";
import type { RouteRecordRaw } from "vue-router";

const ParameterBatchPage = () => import("./features/parameters/ParameterBatchPage.vue");

export const routes: RouteRecordRaw[] = [
  { path: "", component: ParameterBatchPage, meta: { sidebar: "main", returnable: true } },
  {
    path: "settings",
    component: LiliaSettingsPage,
    meta: { sidebar: "settings", lockSidebar: true, returnable: false },
  },
];
