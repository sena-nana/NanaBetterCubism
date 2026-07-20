<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import UiImageViewer from "@lilia/image-viewer/components/ImageViewer";
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
  confirmConversationTurn,
  conversationOnly,
  failConversationTurn,
  installConversationRuntimeStore,
} from "../agent/conversationRuntimeStore";
import { ensureSidebarConversationsLoaded } from "../agent/sidebarConversations";
import type { ChatImageDraft } from "../agent/types";
import { useChatImageDrafts } from "../agent/useChatImageDrafts";

const router = useRouter();
const llm = useLlmConfigStore();
const draft = ref("");
const sending = ref(false);
const loading = ref(true);
const error = ref<string | null>(null);
const imageDrafts = ref<ChatImageDraft[]>([]);
const canCompose = computed(() => llm.state.config.hasApiKey && !sending.value);
const imageDraftController = useChatImageDrafts({
  drafts: imageDrafts,
  canInteract: () => canCompose.value,
  setError: (message) => {
    error.value = message;
  },
});
const viewingImage = imageDraftController.viewingImage;

const canSend = computed(
  () => Boolean(draft.value.trim() || imageDrafts.value.length) && canCompose.value,
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
  if (!canSend.value) return;
  const images = [...imageDrafts.value];
  sending.value = true;
  error.value = null;
  try {
    const created = await createConversation();
    const optimisticId = beginConversationTurn(created.id, content, images);
    try {
      const persisted = await sendMessage(
        created.id,
        content,
        images.map((image) => image.draftId),
        conversationOnly.value,
      );
      confirmConversationTurn(created.id, optimisticId, persisted);
    } catch (err) {
      failConversationTurn(
        created.id,
        optimisticId,
        content,
        images,
        normalizeCommandError(err).message,
      );
      throw err;
    }
    draft.value = "";
    imageDrafts.value = [];
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
  <ConversationSurface
    data-agent-id="home.page"
    agent-id-prefix="agent.home"
    :drop-enabled="canCompose"
    @drop-paths="imageDraftController.addPaths"
  >
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
          :images="imageDrafts"
          :error="error"
          @send="startConversation"
          @pick-images="imageDraftController.pickImages"
          @remove-image="imageDraftController.removeImage"
          @view-image="imageDraftController.viewImage"
          @paste="imageDraftController.pasteImages"
        />
      </div>
    </template>
  </ConversationSurface>
  <UiImageViewer
    v-if="viewingImage"
    :source="viewingImage"
    agent-id="agent.home.image-viewer"
    @close="viewingImage = null"
  />
</template>
