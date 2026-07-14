<script setup lang="ts">
import { UiButton, UiCard, UiEmptyState, UiSwitch } from "@lilia/ui";
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  listMemories,
  listProjects,
  normalizeCommandError,
  setMemoryEnabled,
} from "./bridge";
import type { MemoryRecord, ProjectRecord } from "./types";

const route = useRoute();
const router = useRouter();
const memories = ref<MemoryRecord[]>([]);
const projects = ref<ProjectRecord[]>([]);
const projectFilter = ref<string>("");
const error = ref<string | null>(null);
const loading = ref(true);

const projectMemories = computed(() =>
  memories.value.filter((item) => item.scope === "project" && item.kind === "stage"),
);
const globalMemories = computed(() =>
  memories.value.filter((item) => item.scope === "global" && item.kind === "experience"),
);

onMounted(async () => {
  projects.value = await listProjects();
  const fromQuery = typeof route.query.project === "string" ? route.query.project : "";
  projectFilter.value = fromQuery;
  await reload();
});

watch(projectFilter, async (value) => {
  await router.replace({ query: value ? { project: value } : {} });
  await reload();
});

async function reload() {
  loading.value = true;
  error.value = null;
  try {
    memories.value = await listMemories(projectFilter.value || null);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    loading.value = false;
  }
}

async function toggle(memory: MemoryRecord, enabled: boolean) {
  try {
    await setMemoryEnabled(memory.id, enabled);
    memory.enabled = enabled;
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  }
}
</script>

<template>
  <section class="page agent-memory" data-agent-id="agent.memory">
    <header class="page-header">
      <h1>记忆</h1>
      <p>查看项目阶段记忆与全局 Live2D 经验。</p>
    </header>

    <div class="filter-row">
      <label for="memory-project-filter">项目</label>
      <select
        id="memory-project-filter"
        v-model="projectFilter"
        data-agent-id="agent.memory.project-filter"
      >
        <option value="">全部项目</option>
        <option v-for="project in projects" :key="project.id" :value="project.id">
          {{ project.name }}
        </option>
      </select>
      <UiButton agent-id="agent.memory.reload" @click="reload">刷新</UiButton>
    </div>

    <UiEmptyState
      v-if="loading"
      title="正在加载记忆"
      description="读取本地记忆库。"
      agent-id="agent.memory.loading"
    />

    <template v-else>
      <UiCard title="项目阶段记忆" agent-id="agent.memory.project-section">
        <UiEmptyState
          v-if="!projectMemories.length"
          title="暂无项目阶段记忆"
          description="在对话中绑定项目并整理记忆后会出现在这里。"
          agent-id="agent.memory.project-empty"
        />
        <article
          v-for="memory in projectMemories"
          :key="memory.id"
          class="memory-item"
          :data-agent-id="`agent.memory.item.${memory.id}`"
        >
          <div class="memory-item__head">
            <strong>{{ memory.title }}</strong>
            <UiSwitch
              :model-value="memory.enabled"
              label="启用"
              :agent-id="`agent.memory.enable.${memory.id}`"
              @update:model-value="(v) => toggle(memory, Boolean(v))"
            />
          </div>
          <p class="memory-item__meta">
            {{ memory.projectName ?? "未命名项目" }} · {{ memory.updatedAt }}
            <template v-if="memory.sourceConversationId">
              ·
              <RouterLink :to="`/chats/${memory.sourceConversationId}`">来源对话</RouterLink>
            </template>
          </p>
          <p class="memory-item__body">{{ memory.body }}</p>
        </article>
      </UiCard>

      <UiCard title="全局 Live2D 经验" agent-id="agent.memory.global-section">
        <UiEmptyState
          v-if="!globalMemories.length"
          title="暂无全局经验"
          description="整理记忆时提炼的可迁移经验会显示在这里。"
          agent-id="agent.memory.global-empty"
        />
        <article
          v-for="memory in globalMemories"
          :key="memory.id"
          class="memory-item"
          :data-agent-id="`agent.memory.item.${memory.id}`"
        >
          <div class="memory-item__head">
            <strong>{{ memory.title }}</strong>
            <UiSwitch
              :model-value="memory.enabled"
              label="启用"
              :agent-id="`agent.memory.enable.${memory.id}`"
              @update:model-value="(v) => toggle(memory, Boolean(v))"
            />
          </div>
          <p class="memory-item__meta">{{ memory.updatedAt }}</p>
          <p class="memory-item__body">{{ memory.body }}</p>
        </article>
      </UiCard>
    </template>

    <p v-if="error" class="error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.filter-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.filter-row label {
  color: var(--text-muted);
  font-size: 12px;
}
.filter-row select {
  min-width: 180px;
}
.memory-item {
  padding: 10px 0;
  border-top: 1px solid var(--border-soft);
}
.memory-item:first-of-type {
  border-top: 0;
  padding-top: 0;
}
.memory-item__head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
}
.memory-item__meta {
  margin: 4px 0 8px;
  color: var(--text-faint);
  font-size: 11px;
}
.memory-item__body {
  margin: 0;
  white-space: pre-wrap;
  font-size: 13px;
  line-height: 1.5;
}
.error {
  color: var(--err);
  font-size: 12px;
}
</style>
