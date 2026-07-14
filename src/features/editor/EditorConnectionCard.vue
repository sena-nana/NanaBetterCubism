<script setup lang="ts">
import { UiButton, UiCard, UiInput } from "@lilia/ui";
import { computed, onMounted, ref } from "vue";
import { readIntegerStorage } from "../../utils/storage";
import { useEditorStore } from "./editorStore";

const props = withDefaults(
  defineProps<{
    agentIdPrefix: string;
    disconnectDisabled?: boolean;
  }>(),
  { disconnectDisabled: false },
);

const PORT_STORAGE_KEY = "nanabettercubism.editor-port";
const editor = useEditorStore();
const port = ref(readIntegerStorage(PORT_STORAGE_KEY, 22033, 1, 65535));
const portInputId = computed(() => `${props.agentIdPrefix.replace(/\./g, "-")}-port`);

onMounted(() => {
  void editor.initialize();
});

async function connect() {
  const value = Number(port.value);
  if (!Number.isInteger(value) || value < 1 || value > 65535) return;
  localStorage.setItem(PORT_STORAGE_KEY, String(value));
  await editor.connect(value);
}
</script>

<template>
  <UiCard title="Editor 连接" :agent-id="agentIdPrefix">
    <div class="connection-row">
      <div class="field field--port">
        <label :for="portInputId">本机端口</label>
        <UiInput
          :id="portInputId"
          v-model="port"
          type="number"
          :agent-id="`${agentIdPrefix}.port`"
        />
      </div>
      <UiButton
        v-if="editor.state.snapshot.state === 'disconnected' || editor.state.snapshot.state === 'failed'"
        variant="primary"
        :busy="editor.state.busy"
        :agent-id="`${agentIdPrefix}.connect`"
        @click="connect"
      >
        连接 Editor
      </UiButton>
      <UiButton
        v-else
        :disabled="disconnectDisabled"
        :agent-id="`${agentIdPrefix}.disconnect`"
        @click="editor.disconnect"
      >
        断开
      </UiButton>
      <div class="connection-meta">
        <span>地址 127.0.0.1</span>
        <span>API {{ editor.state.snapshot.apiVersion ?? "-" }}</span>
        <span>{{ editor.state.snapshot.modelLabel ?? "未选择模型" }}</span>
      </div>
    </div>
    <p v-if="editor.state.error" class="message message--error" role="alert">
      {{ editor.state.error.message }}
    </p>
    <p
      v-else-if="editor.state.snapshot.state === 'awaiting_access' || editor.state.snapshot.state === 'awaiting_edit_permission'"
      class="message message--warning"
    >
      保持此页面打开，在 Cubism Editor 的“外部应用联动设置”中完成授权。
    </p>
    <p v-else class="message">{{ editor.state.snapshot.message }}</p>
  </UiCard>
</template>

<style scoped>
.connection-row { display: flex; align-items: flex-end; gap: 8px; flex-wrap: wrap; }
.connection-meta { display: flex; gap: 12px; margin-left: auto; padding-bottom: 7px; color: var(--text-faint); font-size: 11px; }
.field { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
.field--port { width: 130px; }
.field label { color: var(--text-muted); font-size: 11px; font-weight: 600; }
.message { margin: 8px 0 0; font-size: 12px; color: var(--text-muted); }
.message--error { color: var(--err); }
.message--warning { color: var(--warn); }
</style>
