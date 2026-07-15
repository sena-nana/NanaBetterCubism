<script setup lang="ts">
import type { InlineToken } from "./parser";

defineProps<{ tokens: InlineToken[] }>();
const emit = defineEmits<{ openLink: [href: string] }>();
</script>

<template>
  <template v-for="(token, index) in tokens" :key="`${token.type}:${index}`">
    <code v-if="token.type === 'code'">{{ token.text }}</code>
    <strong v-else-if="token.type === 'strong'">{{ token.text }}</strong>
    <em v-else-if="token.type === 'em'">{{ token.text }}</em>
    <br v-else-if="token.type === 'break'">
    <button
      v-else-if="token.type === 'link' && token.href"
      type="button"
      class="markdown-link"
      :data-agent-id="`agent.chat.markdown.link.${index}`"
      @click="emit('openLink', token.href)"
    >{{ token.text }}</button>
    <template v-else>{{ token.text }}</template>
  </template>
</template>
