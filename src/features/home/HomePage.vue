<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import {
  createConversation,
  normalizeCommandError,
  sendMessage,
} from "../agent/bridge";
import ConversationComposer from "../agent/components/ConversationComposer.vue";
import ConversationSurface from "../agent/components/ConversationSurface.vue";
import ConversationTranscript from "../agent/components/ConversationTranscript.vue";
import { useLlmConfigStore } from "../agent/llmConfigStore";
import {
  beginConversationTurn,
  conversationOnly,
  failConversationTurn,
  installConversationRuntimeStore,
} from "../agent/conversationRuntimeStore";
import { ensureSidebarConversationsLoaded } from "../agent/sidebarConversations";

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
      await sendMessage(created.id, content, conversationOnly.value);
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
  <ConversationSurface data-agent-id="home.page">
    <ConversationTranscript
      data-agent-id="home.header"
      :messages="[]"
      :loading="loading"
      :running="sending"
      agent-id-prefix="agent.home"
      empty-title="想在 Cubism Editor 中完成什么？"
      empty-description="输入目标开始新对话；会话会在首次发送时创建。"
    />

    <template #composer>
      <div data-agent-id="home.start-card">
        <ConversationComposer
          v-model="draft"
          v-model:conversation-only="conversationOnly"
          agent-id-prefix="agent.home"
          :disabled="!llm.state.config.hasApiKey"
          :running="sending"
          :can-send="canSend"
          :error="error"
          @send="startConversation"
        />
      </div>
    </template>
  </ConversationSurface>
</template>
