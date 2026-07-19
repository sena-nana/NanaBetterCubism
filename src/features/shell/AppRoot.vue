<script setup lang="ts">
import PanelLeft from "@lucide/vue/dist/esm/icons/panel-left.mjs";
import { computed, type Component } from "vue";
import { RouterView, useRoute } from "vue-router";
import { settingsModel } from "../../app.config";
import {
  APP_METADATA,
  IconButton,
  LiliaPrimaryContent,
  LiliaSectionNavigation,
  LiliaSettingsSidebar,
  LiliaWorkspace,
  SIDEBAR_CONFIG,
  normalizeSettingsTab,
  useNativeAppearance,
  usePersistentBoolean,
  usePersistentNumber,
  useRouteReturnTarget,
  useTheme,
} from "../../ui";
import { appUIPreset } from "../../ui/preset";
import AppSidebar from "./AppSidebar.vue";

const AppProvider = appUIPreset.provider as Component;
const AppShell = appUIPreset.shell as Component;
const route = useRoute();
const { theme } = useTheme();
const appearance = useNativeAppearance();
const { returnTo } = useRouteReturnTarget();
const storedSidebarCollapsed = usePersistentBoolean(SIDEBAR_CONFIG.collapsedStorageKey, false);
const sidebarSize = usePersistentNumber({
  key: SIDEBAR_CONFIG.widthStorageKey,
  defaultValue: SIDEBAR_CONFIG.defaultWidth,
  min: SIDEBAR_CONFIG.minWidth,
  max: SIDEBAR_CONFIG.maxWidth,
});

const settingsMode = computed(() => route.meta.sidebar === "settings");
const sidebarCollapsed = computed({
  get: () => settingsMode.value ? false : storedSidebarCollapsed.value,
  set: (collapsed: boolean) => {
    if (!settingsMode.value) storedSidebarCollapsed.value = collapsed;
  },
});
const activeSettingsTab = computed(() => normalizeSettingsTab(settingsModel, route.query.tab));
const translucent = computed(() => appearance.backdropMode.value !== "solid");
const workspaceSurfaceMode = computed(() => translucent.value ? "translucent" : "solid");
const sidebarSurfaceMode = computed(() => (
  translucent.value && appearance.backdropTarget.value === "sidebar" ? "translucent" : "solid"
));
const primarySurfaceMode = computed(() => (
  translucent.value && appearance.backdropTarget.value === "main" ? "translucent" : "solid"
));

function toggleSidebar() {
  if (!settingsMode.value) sidebarCollapsed.value = !sidebarCollapsed.value;
}
</script>

<template>
  <component
    :is="AppProvider"
    class="nanabettercubism-root"
    :policy="appUIPreset.policy"
    :theme="theme"
    agent-id="app.provider"
  >
    <component :is="AppShell" :title="APP_METADATA.productTitle" agent-id="shell">
      <template #header-leading>
        <IconButton
          :icon="PanelLeft"
          :label="sidebarCollapsed ? '展开侧栏' : '收起侧栏'"
          :active="!sidebarCollapsed"
          :disabled="settingsMode"
          agent-id="shell.sidebar.toggle"
          @click="toggleSidebar"
        />
      </template>

      <LiliaWorkspace
        aria-label="NanaBetterCubism 工作区"
        agent-id="shell.workspace"
        :surface-mode="workspaceSurfaceMode"
      >
        <LiliaSectionNavigation
          id="navigation"
          v-model:collapsed="sidebarCollapsed"
          v-model:size="sidebarSize"
          as="div"
          role="section-navigation"
          :collapsible="!settingsMode"
          resizable
          :default-size="SIDEBAR_CONFIG.defaultWidth"
          :min-size="SIDEBAR_CONFIG.minWidth"
          :max-size="SIDEBAR_CONFIG.maxWidth"
          :surface-mode="sidebarSurfaceMode"
          backdrop-effect="none"
          resize-label="调整侧栏宽度"
        >
          <LiliaSettingsSidebar
            v-if="settingsMode"
            :tabs="settingsModel.tabs"
            :active-key="activeSettingsTab"
            :return-to="returnTo"
            :surface-mode="sidebarSurfaceMode"
          />
          <AppSidebar v-else :surface-mode="sidebarSurfaceMode" />
        </LiliaSectionNavigation>

        <LiliaPrimaryContent
          id="primary"
          role="primary"
          :surface-mode="primarySurfaceMode"
          backdrop-effect="none"
        >
          <RouterView />
        </LiliaPrimaryContent>
      </LiliaWorkspace>
    </component>
  </component>
</template>

<style scoped>
.nanabettercubism-root {
  height: 100%;
  min-height: 0;
}
</style>
