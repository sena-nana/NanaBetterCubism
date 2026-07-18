<script setup lang="ts">
import {
  IconButton,
  LiliaSidebarFrame,
  LiliaSidebarNavRow,
  LiliaSidebarSection,
  SIDEBAR_GROUPS,
  SIDEBAR_NAV,
  SIDEBAR_TOP_CONTENT,
  type SidebarActionItem,
} from "../../ui";

function selectAction(action: SidebarActionItem) {
  if (action.disabled || !action.onSelect) return;
  void action.onSelect();
}
</script>

<template>
  <LiliaSidebarFrame aria-label="主导航">
    <template #top>
      <div class="app-sidebar__top">
        <component v-if="SIDEBAR_TOP_CONTENT" :is="SIDEBAR_TOP_CONTENT" />
        <nav v-if="SIDEBAR_NAV.length" class="app-sidebar__navigation" aria-label="主导航">
          <LiliaSidebarNavRow
            v-for="item in SIDEBAR_NAV"
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
