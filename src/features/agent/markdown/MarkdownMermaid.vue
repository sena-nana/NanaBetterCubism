<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  MERMAID_MANUAL_ACTIVATION_LENGTH,
  renderMermaid,
  validateMermaidSource,
} from "./mermaidRenderer";

const props = defineProps<{ source: string }>();
const root = ref<HTMLElement | null>(null);
const svg = ref("");
const error = ref("");
const visible = ref(false);
const activated = ref(false);
const requiresActivation = computed(
  () => props.source.length > MERMAID_MANUAL_ACTIVATION_LENGTH,
);
let observer: IntersectionObserver | null = null;
let renderEpoch = 0;

async function render() {
  const epoch = ++renderEpoch;
  svg.value = "";
  error.value = "";
  try {
    validateMermaidSource(props.source);
    if (!visible.value || (requiresActivation.value && !activated.value)) return;
    const result = await renderMermaid(props.source);
    if (epoch === renderEpoch) svg.value = result;
  } catch (reason) {
    if (epoch === renderEpoch) {
      error.value = reason instanceof Error ? reason.message : "图表渲染失败。";
    }
  }
}

function activate() {
  activated.value = true;
  void render();
}

watch(() => props.source, () => {
  activated.value = false;
  void render();
});

onMounted(() => {
  if (typeof IntersectionObserver === "undefined") {
    visible.value = true;
    void render();
    return;
  }
  observer = new IntersectionObserver((entries) => {
    if (!entries.some((entry) => entry.isIntersecting)) return;
    visible.value = true;
    observer?.disconnect();
    observer = null;
    void nextTick(render);
  }, { rootMargin: "160px" });
  if (root.value) observer.observe(root.value);
});

onBeforeUnmount(() => {
  renderEpoch += 1;
  observer?.disconnect();
});
</script>

<template>
  <section ref="root" class="markdown-mermaid">
    <div v-if="svg" class="markdown-mermaid__canvas" v-html="svg" />
    <button
      v-else-if="requiresActivation && !activated && !error"
      type="button"
      class="markdown-mermaid__activate"
      @click="activate"
    >
      显示图表
    </button>
    <div v-else-if="!error" class="markdown-mermaid__loading">正在生成图表…</div>
    <template v-if="error">
      <p class="markdown-mermaid__error" role="alert">{{ error }}</p>
      <pre class="markdown-mermaid__source"><code>{{ source }}</code></pre>
    </template>
  </section>
</template>

<style scoped>
.markdown-mermaid { min-height: 54px; margin: 0 0 10px; overflow: auto; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); background: var(--bg-subtle); }
.markdown-mermaid__canvas { display: grid; place-items: center; min-width: max-content; padding: 12px; color: var(--text); }
.markdown-mermaid__canvas :deep(svg) { display: block; max-width: min(100%, 920px); height: auto; }
.markdown-mermaid__activate { display: block; margin: 12px auto; padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-elev); color: var(--text); cursor: pointer; }
.markdown-mermaid__loading, .markdown-mermaid__error { margin: 0; padding: 12px; color: var(--text-muted); font-size: 12px; text-align: center; }
.markdown-mermaid__error { color: var(--err); }
.markdown-mermaid__source { margin: 0; padding: 10px 12px; overflow: auto; border-top: 1px solid var(--border-soft); white-space: pre; }
</style>
