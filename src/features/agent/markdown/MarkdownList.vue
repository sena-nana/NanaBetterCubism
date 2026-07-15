<script setup lang="ts">
import type { MarkdownListNode } from "./parser";
import MarkdownInline from "./MarkdownInline.vue";

defineProps<{ list: MarkdownListNode }>();
const emit = defineEmits<{ openLink: [href: string] }>();
</script>

<template>
  <component
    :is="list.ordered ? 'ol' : 'ul'"
    class="markdown-list"
    :start="list.ordered && list.start !== null ? list.start : undefined"
  >
    <li
      v-for="(item, itemIndex) in list.items"
      :key="itemIndex"
      :class="{ 'markdown-list__item--task': item.taskChecked !== null }"
    >
      <div class="markdown-list__content">
        <input
          v-if="item.taskChecked !== null"
          type="checkbox"
          :checked="item.taskChecked"
          disabled
          :data-agent-id="`agent.chat.markdown.task.${itemIndex}`"
          :aria-label="item.taskChecked ? '已完成' : '未完成'"
        >
        <span><MarkdownInline :tokens="item.inlines" @open-link="emit('openLink', $event)" /></span>
      </div>
      <MarkdownList
        v-for="(child, childIndex) in item.children"
        :key="childIndex"
        :list="child"
        @open-link="emit('openLink', $event)"
      />
    </li>
  </component>
</template>
