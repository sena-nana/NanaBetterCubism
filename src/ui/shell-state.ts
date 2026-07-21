import { reactive } from "vue";
import {
  SIDEBAR_FOOTER_STATUSES,
  type SidebarActionItem,
  type SidebarNavItem,
} from "@lilia/ui/shell";

export { SIDEBAR_FOOTER_STATUSES, type SidebarNavItem };

export interface SidebarGroup {
  emptyText?: string;
  items?: SidebarNavItem[];
  key: string;
  title: string;
  tools?: SidebarActionItem[];
}

export const SIDEBAR_GROUPS = reactive<SidebarGroup[]>([]);

export const SIDEBAR_CONFIG = {
  widthStorageKey: "nanabettercubism.sidebarWidth",
  collapsedStorageKey: "nanabettercubism.sidebarCollapsed",
  minWidth: 180,
  maxWidth: 480,
  defaultWidth: 220,
} as const;
