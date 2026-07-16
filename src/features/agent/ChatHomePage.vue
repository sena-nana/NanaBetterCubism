<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  createConversation,
  normalizeCommandError,
  sendMessage,
} from "./bridge";
import ConversationComposer from "./components/ConversationComposer.vue";
import ConversationSurface from "./components/ConversationSurface.vue";
import ConversationTranscript from "./components/ConversationTranscript.vue";
import { useLlmConfigStore } from "./llmConfigStore";
import {
  beginConversationTurn,
  failConversationTurn,
  installConversationRuntimeStore,
} from "./conversationRuntimeStore";
import { ensureSidebarConversationsLoaded } from "./sidebarConversations";

const router = useRouter();
const llm = useLlmConfigStore();
const draft = ref("");
const sending = ref(false);
const loading = ref(true);
const error = ref<string | null>(null);

const canSend = computed(
  () => Boolean(draft.value.trim()) && llm.state.config.hasApiKey && !sending.value,
);

onMounted(async () => {
  try {
    await Promise.all([
      installConversationRuntimeStore(),
      llm.initialize(),
    ]);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    loading.value = false;
  }
});

async function startConversation() {
  const content = draft.value.trim();
  if (!content || !canSend.value) return;
  sending.value = true;
  error.value = null;
  try {
    const created = await createConversation();
    const optimisticId = beginConversationTurn(created.id, content);
    try {
      await sendMessage(created.id, content);
    } catch (err) {
      failConversationTurn(
        created.id,
        optimisticId,
        content,
        normalizeCommandError(err).message,
      );
      throw err;
    }
    draft.value = "";
    await ensureSidebarConversationsLoaded(true);
    await router.push(`/chats/${created.id}`);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    sending.value = false;
  }
}

</script>

<template>
  <ConversationSurface data-agent-id="agent.home">
    <ConversationTranscript
      :messages="[]"
      :loading="loading"
      :running="sending"
      agent-id-prefix="agent.home"
      empty-title="想在 Cubism Editor 中完成什么？"
      empty-description="输入目标开始新对话；会话会在首次发送时创建。"
    />

    <template #composer>
      <ConversationComposer
        v-model="draft"
        agent-id-prefix="agent.home"
        :disabled="!llm.state.config.hasApiKey"
        :running="sending"
        :can-send="canSend"
        :error="error"
        @send="startConversation"
      />
    </template>
  </ConversationSurface>
</template>
