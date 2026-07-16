<script setup lang="ts">
import ContextMenuHost from "@lilia/ui/components/ContextMenuHost";
import OverlayHost from "@lilia/ui/components/OverlayHost";
import { RouterView } from "vue-router";
import { appOverlays } from "./overlays";
</script>

<template>
  <RouterView />
  <ContextMenuHost />
  <OverlayHost :components="appOverlays" />
</template>
