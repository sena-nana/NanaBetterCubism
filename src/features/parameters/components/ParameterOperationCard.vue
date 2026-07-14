<script setup lang="ts">
import { UiButton, UiCard } from "@lilia/ui";
import { computed } from "vue";
import type { BatchFinished, BatchProgress, EditorConnectionState } from "../types";

const props = defineProps<{
  active: boolean;
  state: EditorConnectionState;
  progress: BatchProgress | null;
  finished: BatchFinished | null;
}>();
const emit = defineEmits<{ cancel: [] }>();

const progressPercent = computed(() => {
  if (!props.progress?.total) return 0;
  return Math.round((props.progress.completed / props.progress.total) * 100);
});
</script>

<template>
  <UiCard title="执行状态" agent-id="parameters.operation">
    <div v-if="active" class="progress-row">
      <progress :value="progressPercent" max="100" data-agent-id="parameters.operation.progress"></progress>
      <span>{{ progressPercent }}%</span>
      <code>{{ progress?.currentId ?? "准备中" }}</code>
      <UiButton
        variant="warning"
        :disabled="state === 'cancelling'"
        agent-id="parameters.operation.cancel"
        @click="emit('cancel')"
      >
        {{ state === "cancelling" ? "正在取消" : "取消并恢复" }}
      </UiButton>
    </div>
    <p v-else-if="finished" class="message" :class="finished.outcome === 'committed' ? 'message--success' : 'message--warning'">
      {{ finished.message }}
    </p>
  </UiCard>
</template>

<style scoped>
.progress-row { display: flex; align-items: center; gap: 8px; }
.progress-row progress { flex: 1 1 auto; height: 7px; accent-color: var(--accent); }
.progress-row > span { width: 38px; color: var(--text-muted); font-size: 11px; }
.progress-row code { width: 180px; overflow: hidden; color: var(--accent); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
.message { margin: 0; font-size: 12px; }
.message--warning { color: var(--warn); }
.message--success { color: var(--ok); }
</style>
