<script setup lang="ts">
import { UiButton } from "@lilia/ui";
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { useEditorStore } from "../editor/editorStore";
import {
  createConversation,
  getLlmConfig,
  normalizeCommandError,
  sendMessage,
} from "./bridge";
import ConversationComposer from "./components/ConversationComposer.vue";
import ConversationSurface from "./components/ConversationSurface.vue";
import ConversationTranscript from "./components/ConversationTranscript.vue";
import { editorStatusLabel, modelStatusLabel } from "./conversationPresentation";
import { setConversationTurnPhase } from "./conversationRuntimeStore";
import { ensureSidebarConversationsLoaded } from "./sidebarConversations";
import type { LlmConfigView } from "./types";

const router = useRouter();
const editor = useEditorStore();
const draft = ref("");
const sending = ref(false);
const loading = ref(true);
const error = ref<string | null>(null);
const llm = ref<LlmConfigView>({ baseUrl: null, model: null, hasApiKey: false });

const canSend = computed(
  () => Boolean(draft.value.trim()) && llm.value.hasApiKey && !sending.value,
);

onMounted(async () => {
  void editor.initialize();
  try {
    llm.value = await getLlmConfig();
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
    setConversationTurnPhase(created.id, "running");
    try {
      await sendMessage(created.id, content);
    } catch (err) {
      setConversationTurnPhase(created.id, "idle");
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

function goSettings(tab: string) {
  void router.push(`/settings?tab=${tab}`);
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
        :disabled="!llm.hasApiKey"
        :running="sending"
        :can-send="canSend"
        :error="error"
        @send="startConversation"
      >
        <template #toolbar>
          <UiButton
            size="sm"
            agent-id="agent.home.model-settings"
            @click="goSettings('model-config')"
          >
            {{ modelStatusLabel(llm) }}
          </UiButton>
          <UiButton
            size="sm"
            agent-id="agent.home.editor-settings"
            @click="goSettings('editor')"
          >
            {{ editorStatusLabel(editor.state.snapshot.state) }}
          </UiButton>
        </template>
      </ConversationComposer>
    </template>
  </ConversationSurface>
</template>
