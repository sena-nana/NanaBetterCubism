import { LiliaSettingsPage } from "@lilia/ui";
import type { RouteRecordRaw } from "vue-router";

const ParameterBatchPage = () => import("./features/parameters/ParameterBatchPage.vue");
const PartParametersPage = () => import("./features/part-parameters/PartParametersPage.vue");

export const routes: RouteRecordRaw[] = [
  { path: "", component: ParameterBatchPage, meta: { sidebar: "main", returnable: true } },
  {
    path: "part-parameters",
    component: PartParametersPage,
    meta: { sidebar: "part-parameters", returnable: true },
  },
  {
    path: "settings",
    component: LiliaSettingsPage,
    meta: { sidebar: "settings", lockSidebar: true, returnable: false },
  },
];
