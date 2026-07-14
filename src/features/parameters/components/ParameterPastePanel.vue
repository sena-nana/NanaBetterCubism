<script setup lang="ts">
import { UiButton, UiTextarea } from "@lilia/ui";
import { ref } from "vue";
import type { ParameterInputRow } from "../types";
import { parsePastedRows } from "../utils/pasteRows";

const props = defineProps<{ currentCount: number; replaceBlankRow: boolean }>();
const emit = defineEmits<{
  close: [];
  import: [rows: ParameterInputRow[]];
}>();

const value = ref("");
const error = ref("");

function importRows() {
  const rows = parsePastedRows(value.value);
  if (!rows.length) {
    error.value = "没有识别到参数行。";
    return;
  }
  const retainedCount = props.replaceBlankRow ? 0 : props.currentCount;
  if (retainedCount + rows.length > 200) {
    error.value = `导入后将有 ${retainedCount + rows.length} 行，单批最多 200 行。`;
    return;
  }
  emit("import", rows);
}
</script>

<template>
  <div class="paste-panel" data-agent-id="parameters.paste.panel">
    <UiTextarea
      v-model="value"
      placeholder="前发摆动,Hair,L&#10;后发摆动,Hair,R"
      agent-id="parameters.paste.input"
    />
    <div class="paste-actions">
      <span v-if="error" class="message message--error">{{ error }}</span>
      <UiButton size="sm" agent-id="parameters.paste.cancel" @click="emit('close')">取消</UiButton>
      <UiButton variant="primary" size="sm" agent-id="parameters.paste.import" @click="importRows">导入</UiButton>
    </div>
  </div>
</template>

<style scoped>
.paste-panel { margin-bottom: 10px; padding: 10px; border: 1px solid var(--border-soft); border-radius: var(--radius-md); background: var(--bg-subtle); }
.paste-panel :deep(textarea) { min-height: 104px; resize: vertical; }
.paste-actions { display: flex; justify-content: flex-end; align-items: center; gap: 8px; margin-top: 8px; }
.paste-actions .message { margin-right: auto; }
.message { margin: 0; font-size: 12px; }
.message--error { color: var(--err); }
</style>
