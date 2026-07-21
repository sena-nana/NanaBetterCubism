<script setup lang="ts">
import Brain from "@lucide/vue/dist/esm/icons/brain.mjs";
import House from "@lucide/vue/dist/esm/icons/house.mjs";
import ConversationSidebarTop from "../agent/components/ConversationSidebarTop.vue";
import type { SurfaceMode } from "../../ui/contract";
import {
  IconButton,
  LiliaSidebarFrame,
  LiliaSidebarNavRow,
  LiliaSidebarSection,
  type SidebarActionItem,
  type SidebarNavItem,
} from "../../ui";
import { SIDEBAR_GROUPS } from "../../ui/shell-state";

const { surfaceMode } = defineProps<{
  surfaceMode: SurfaceMode;
}>();

const navItems: SidebarNavItem[] = [
  { key: "home", to: "/", label: "首页", icon: House },
  { key: "memory", to: "/memory", label: "记忆", icon: Brain },
];

function selectAction(action: SidebarActionItem) {
  if (action.disabled || !action.onSelect) return;
  void action.onSelect();
}
</script>

<template>
  <LiliaSidebarFrame aria-label="主导航" :surface-mode="surfaceMode">
    <template #top>
      <div class="app-sidebar__top">
        <ConversationSidebarTop />
        <nav class="app-sidebar__navigation" aria-label="主导航">
          <LiliaSidebarNavRow
            v-for="item in navItems"
            :key="item.key"
            :item="item"
            :agent-id="`sidebar.nav.${item.key}`"
            :emphasis="item.emphasis"
          />
        </nav>
      </div>
    </template>

    <template #body>
      <LiliaSidebarSection
        v-for="group in SIDEBAR_GROUPS"
        :key="group.key"
        :title="group.title"
        :collapsible="false"
        :agent-id="`sidebar.group.${group.key}`"
      >
        <template v-if="group.tools?.length" #tools>
          <IconButton
            v-for="tool in group.tools"
            :key="tool.key"
            size="sm"
            :icon="tool.icon"
            :label="tool.label"
            :active="tool.active"
            :disabled="tool.disabled || !tool.onSelect"
            :agent-id="`sidebar.group.${group.key}.tool.${tool.key}`"
            @click="selectAction(tool)"
          />
        </template>
        <LiliaSidebarNavRow
          v-for="item in group.items"
          :key="item.key"
          :item="item"
          :agent-id="`sidebar.group.${group.key}.item.${item.key}`"
          :emphasis="item.emphasis"
        />
        <p v-if="!group.items?.length && group.emptyText" class="app-sidebar__empty">
          {{ group.emptyText }}
        </p>
      </LiliaSidebarSection>
    </template>
  </LiliaSidebarFrame>
</template>

<style scoped>
.app-sidebar__top,
.app-sidebar__navigation {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.app-sidebar__top {
  gap: 10px;
}

.app-sidebar__empty {
  margin: 6px 8px;
  color: var(--text-faint);
  font-size: 12px;
}
</style>
