<script setup lang="ts">
import { UiButton, UiEmptyState } from "@lilia/ui";
import { onMounted, ref } from "vue";
import { useRouter } from "vue-router";
import { createConversation, listConversations, normalizeCommandError } from "./bridge";
import { ensureSidebarConversationsLoaded } from "./sidebarConversations";

const router = useRouter();
const error = ref<string | null>(null);
const loading = ref(true);

onMounted(async () => {
  try {
    const rows = await listConversations();
    await ensureSidebarConversationsLoaded(true);
    if (rows[0]) {
      await router.replace(`/chats/${rows[0].id}`);
      return;
    }
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    loading.value = false;
  }
});

async function startChat() {
  error.value = null;
  try {
    const created = await createConversation();
    await ensureSidebarConversationsLoaded(true);
    await router.push(`/chats/${created.id}`);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  }
}
</script>

<template>
  <section class="page agent-home" data-agent-id="agent.home">
    <header class="page-header">
      <h1>Cubism Agent</h1>
      <p>通过对话调用 Cubism Editor 工具，并保留项目记忆。</p>
    </header>
    <UiEmptyState
      v-if="loading"
      title="正在加载对话"
      description="读取本地会话列表。"
      agent-id="agent.home.loading"
    />
    <div v-else class="agent-home__body">
      <UiEmptyState
        title="还没有对话"
        description="创建新对话后，Agent 可以查询部件、批量创建参数、截取 Cubism Editor 窗口，并整理项目记忆。"
        agent-id="agent.home.empty"
      />
      <UiButton variant="primary" agent-id="agent.home.new" @click="startChat">新对话</UiButton>
      <p v-if="error" class="agent-home__error" role="alert">{{ error }}</p>
    </div>
  </section>
</template>

<style scoped>
.agent-home__body {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
  padding: 8px 0;
}
.agent-home__error {
  margin: 0;
  color: var(--err);
  font-size: 12px;
}
</style>
