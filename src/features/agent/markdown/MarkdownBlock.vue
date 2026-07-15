<script setup lang="ts">
import { computed, ref, type CSSProperties } from "vue";
import { openExternalUrl } from "../externalLinks";
import MarkdownInline from "./MarkdownInline.vue";
import MarkdownList from "./MarkdownList.vue";
import {
  normalizeMarkdownSource,
  parseMarkdownBlocks,
  type MarkdownBlockNode,
  type TableAlignment,
} from "./parser";

const props = defineProps<{ content: string | null | undefined }>();
const source = computed(() => normalizeMarkdownSource(props.content));
const blocks = computed(() => parseMarkdownBlocks(source.value));
const linkError = ref(false);

function headingTag(block: MarkdownBlockNode) {
  return `h${block.level}` as "h2" | "h3" | "h4" | "h5" | "h6";
}

function alignmentStyle(alignment: TableAlignment): CSSProperties | undefined {
  return alignment ? { textAlign: alignment } : undefined;
}

async function openLink(href: string) {
  linkError.value = false;
  try {
    linkError.value = !(await openExternalUrl(href));
  } catch {
    linkError.value = true;
  }
}
</script>

<template>
  <div v-if="source" class="markdown-block">
    <template v-for="block in blocks" :key="block.key">
      <component :is="headingTag(block)" v-if="block.type === 'heading'" class="markdown-block__heading">
        <MarkdownInline :tokens="block.inlines" @open-link="openLink" />
      </component>
      <pre v-else-if="block.type === 'code'" class="markdown-block__code" :data-language="block.language || undefined"><code>{{ block.text }}</code></pre>
      <div v-else-if="block.type === 'table'" class="markdown-block__table-wrap">
        <table class="markdown-block__table">
          <thead>
            <tr>
              <th v-for="(cell, cellIndex) in block.headers" :key="cellIndex" :style="alignmentStyle(block.alignments[cellIndex] ?? null)">
                <MarkdownInline :tokens="cell" @open-link="openLink" />
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, rowIndex) in block.rows" :key="rowIndex">
              <td v-for="(cell, cellIndex) in row" :key="cellIndex" :style="alignmentStyle(block.alignments[cellIndex] ?? null)">
                <MarkdownInline :tokens="cell" @open-link="openLink" />
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <MarkdownList v-else-if="block.type === 'list' && block.list" :list="block.list" @open-link="openLink" />
      <blockquote v-else-if="block.type === 'quote'" class="markdown-block__quote">
        <MarkdownInline :tokens="block.inlines" @open-link="openLink" />
      </blockquote>
      <p v-else class="markdown-block__paragraph">
        <MarkdownInline :tokens="block.inlines" @open-link="openLink" />
      </p>
    </template>
    <p v-if="linkError" class="markdown-block__error" role="alert">无法打开这个链接。</p>
  </div>
</template>

<style scoped>
.markdown-block { min-width: 0; font-size: 13px; line-height: 1.68; overflow-wrap: anywhere; }
.markdown-block > :first-child { margin-top: 0; }
.markdown-block > :last-child { margin-bottom: 0; }
.markdown-block__paragraph, .markdown-block__heading, .markdown-list, .markdown-block__quote, .markdown-block__code, .markdown-block__table-wrap { margin: 0 0 10px; }
.markdown-block__heading { color: var(--text); font-weight: 600; line-height: 1.35; }
h2.markdown-block__heading { font-size: 17px; }
h3.markdown-block__heading { font-size: 15px; }
h4.markdown-block__heading, h5.markdown-block__heading, h6.markdown-block__heading { font-size: 13px; }
.markdown-block :deep(.markdown-list) { padding-left: 22px; }
.markdown-block :deep(.markdown-list .markdown-list) { margin-top: 4px; margin-bottom: 0; }
.markdown-block :deep(.markdown-list__item--task) { list-style: none; }
.markdown-block :deep(.markdown-list__content) { display: flex; align-items: baseline; gap: 7px; }
.markdown-block :deep(.markdown-list__content input) { margin: 0; accent-color: var(--accent); }
.markdown-block :deep(.markdown-link) { display: inline; padding: 0; border: 0; background: transparent; color: var(--accent); cursor: pointer; font: inherit; text-decoration: underline; text-underline-offset: 2px; }
.markdown-block :deep(code) { padding: 1px 4px; border-radius: var(--radius-xs); background: var(--bg-subtle); font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 0.92em; }
.markdown-block__code { max-width: 100%; padding: 10px 12px; overflow: auto; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); background: var(--bg-subtle); white-space: pre; }
.markdown-block__code code { padding: 0; background: transparent; }
.markdown-block__quote { padding: 3px 0 3px 12px; border-left: 2px solid var(--border-strong); color: var(--text-muted); }
.markdown-block__table-wrap { max-width: 100%; overflow: auto; border: 1px solid var(--border-soft); border-radius: var(--radius-sm); }
.markdown-block__table { width: 100%; border-collapse: collapse; font-size: 12px; }
.markdown-block__table th, .markdown-block__table td { padding: 6px 8px; border-right: 1px solid var(--border-soft); border-bottom: 1px solid var(--border-soft); text-align: left; vertical-align: top; }
.markdown-block__table th { background: var(--bg-subtle); font-weight: 600; }
.markdown-block__table tr:last-child td { border-bottom: 0; }
.markdown-block__table :is(th, td):last-child { border-right: 0; }
.markdown-block__error { margin: 6px 0 0; color: var(--err); font-size: 12px; }
</style>
