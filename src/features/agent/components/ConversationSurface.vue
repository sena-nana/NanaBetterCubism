<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../../editor/bridge";

const props = withDefaults(defineProps<{ dropEnabled?: boolean; agentIdPrefix?: string }>(), {
  dropEnabled: false,
  agentIdPrefix: "agent.chat",
});
const emit = defineEmits<{ dropPaths: [paths: string[]] }>();
const surface = ref<HTMLElement | null>(null);
const dropActive = ref(false);
let unlisten: (() => void) | null = null;

onMounted(async () => {
  if (!isTauriRuntime()) return;
  unlisten = await getCurrentWebview().onDragDropEvent(async ({ payload }) => {
    if (payload.type === "leave") {
      dropActive.value = false;
      return;
    }
    const scale = await getCurrentWindow().scaleFactor();
    const point = { x: payload.position.x / scale, y: payload.position.y / scale };
    const rect = surface.value?.getBoundingClientRect();
    const inside = Boolean(
      rect && point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom,
    );
    const accepts = props.dropEnabled && inside;
    dropActive.value = payload.type !== "drop" && accepts;
    if (payload.type === "drop" && accepts) emit("dropPaths", payload.paths);
  });
});

onUnmounted(() => unlisten?.());
</script>

<template>
  <section ref="surface" class="conversation-surface">
    <main class="conversation-surface__main">
      <slot />
    </main>

    <footer class="conversation-surface__controls">
      <div class="conversation-surface__controls-inner">
        <slot name="context" />
        <slot name="composer" />
      </div>
    </footer>
    <div
      v-if="dropActive"
      class="conversation-surface__drop"
      :data-agent-id="`${agentIdPrefix}.image-drop`"
    >
      <span>释放以添加图片</span>
    </div>
  </section>
</template>

<style scoped>
.conversation-surface {
  position: relative;
  display: grid;
  grid-template-areas:
    "main"
    "controls";
  grid-template-rows: minmax(0, 1fr) auto;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--bg);
}
.conversation-surface__drop { position: absolute; inset: 10px; z-index: 10; display: grid; place-items: center; border: 1px dashed var(--accent); border-radius: var(--radius-md); background: color-mix(in srgb, var(--bg) 88%, transparent); color: var(--text); font-size: 13px; pointer-events: none; }

.conversation-surface__main { grid-area: main; min-width: 0; min-height: 0; }
.conversation-surface__controls { grid-area: controls; position: relative; z-index: 3; min-width: 0; padding: 10px clamp(16px, 5vw, 64px) 14px; background: var(--bg); }
.conversation-surface__controls-inner { display: flex; flex-direction: column; width: min(860px, 100%); margin: 0 auto; }
.conversation-surface__controls-inner :deep(.context-panel) { margin-bottom: 8px; }
.conversation-surface__controls-inner :deep(.plan-todo-panel + .conversation-composer) { border-top-left-radius: 0; border-top-right-radius: 0; border-top-color: var(--border-soft); }

@media (max-width: 720px) {
  .conversation-surface__controls { padding: 8px 12px 10px; }
}
</style>
