<script setup lang="ts">
import { SegmentedControl } from "../../ui";
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  listMemories,
  listProjects,
  normalizeCommandError,
  setMemoryEnabled,
} from "./bridge";
import MemoryDetailPane from "./components/MemoryDetailPane.vue";
import MemoryListPane from "./components/MemoryListPane.vue";
import { matchesMemory } from "./memoryPresentation";
import type { MemoryRecord, MemoryScope, ProjectRecord } from "./types";

const route = useRoute();
const router = useRouter();
const scope = ref<MemoryScope>("project");
const projectId = ref("");
const search = ref("");
const memories = ref<MemoryRecord[]>([]);
const projects = ref<ProjectRecord[]>([]);
const selectedId = ref<string | null>(null);
const loading = ref(true);
const refreshing = ref(false);
const loadError = ref<string | null>(null);
const actionError = ref<string | null>(null);
const togglingId = ref<string | null>(null);
let loadGeneration = 0;
let initializationGeneration = 0;

const scopeOptions = [
  { value: "project", label: "项目记忆", agentId: "agent.memory.scope.project" },
  { value: "global", label: "全局记忆", agentId: "agent.memory.scope.global" },
] as const;

const filteredMemories = computed(() =>
  memories.value.filter((memory) => matchesMemory(memory, search.value)),
);
const selectedMemory = computed(
  () => filteredMemories.value.find((memory) => memory.id === selectedId.value) ?? null,
);

watch(
  filteredMemories,
  (items) => {
    if (!items.some((memory) => memory.id === selectedId.value)) {
      selectedId.value = items[0]?.id ?? null;
    }
  },
  { immediate: true },
);

onMounted(() => void initialize());

async function initialize() {
  const generation = ++initializationGeneration;
  ++loadGeneration;
  loading.value = true;
  refreshing.value = false;
  loadError.value = null;
  try {
    const nextProjects = await listProjects();
    if (generation !== initializationGeneration) return;
    projects.value = nextProjects;
    scope.value = route.query.scope === "global" ? "global" : "project";
    const queryProject = typeof route.query.project === "string" ? route.query.project : "";
    projectId.value = projects.value.some((project) => project.id === queryProject)
      ? queryProject
      : "";
    await syncRoute();
    if (generation !== initializationGeneration) return;
    await loadCurrentScope(false);
  } catch (error) {
    if (generation === initializationGeneration) {
      loadError.value = normalizeCommandError(error).message;
    }
  } finally {
    if (generation === initializationGeneration) {
      loading.value = false;
      refreshing.value = false;
    }
  }
}

async function syncRoute() {
  await router.replace({
    query: {
      scope: scope.value,
      ...(projectId.value ? { project: projectId.value } : {}),
    },
  });
}

async function loadCurrentScope(silent = true, preserveId = selectedId.value) {
  const generation = ++loadGeneration;
  if (silent) refreshing.value = true;
  else loading.value = true;
  loadError.value = null;
  const requestedScope = scope.value;
  const requestedProject = requestedScope === "project" ? projectId.value || null : null;
  try {
    const next = await listMemories(requestedScope, requestedProject);
    if (generation !== loadGeneration) return;
    memories.value = next;
    selectedId.value = next.some((memory) => memory.id === preserveId)
      ? preserveId
      : next[0]?.id ?? null;
  } catch (error) {
    if (generation === loadGeneration) {
      loadError.value = normalizeCommandError(error).message;
    }
  } finally {
    if (generation === loadGeneration) {
      loading.value = false;
      refreshing.value = false;
    }
  }
}

async function changeScope(value: string | number) {
  const next = value === "global" ? "global" : "project";
  if (scope.value === next) return;
  scope.value = next;
  search.value = "";
  selectedId.value = null;
  actionError.value = null;
  await syncRoute();
  await loadCurrentScope(false, null);
}

async function changeProject(value: string) {
  if (projectId.value === value) return;
  projectId.value = value;
  search.value = "";
  selectedId.value = null;
  actionError.value = null;
  await syncRoute();
  await loadCurrentScope(false, null);
}

async function toggleMemory(memory: MemoryRecord, enabled: boolean) {
  if (togglingId.value) return;
  togglingId.value = memory.id;
  actionError.value = null;
  try {
    await setMemoryEnabled(memory.id, enabled);
    await loadCurrentScope(true, memory.id);
  } catch (error) {
    actionError.value = normalizeCommandError(error).message;
  } finally {
    togglingId.value = null;
  }
}
</script>

<template>
  <section class="page agent-memory" data-agent-id="agent.memory">
    <header class="page-header agent-memory__header">
      <div>
        <h1>记忆</h1>
        <p>按项目查看当前进展，或浏览可跨项目复用的经验。</p>
      </div>
      <SegmentedControl
        :model-value="scope"
        :options="scopeOptions"
        aria-label="记忆范围"
        agent-id="agent.memory.scope"
        @update:model-value="changeScope"
      />
    </header>

    <div class="memory-workspace">
      <MemoryListPane
        :scope="scope"
        :projects="projects"
        :project-id="projectId"
        :search="search"
        :memories="filteredMemories"
        :selected-id="selectedId"
        :loading="loading"
        :refreshing="refreshing"
        :error="loadError"
        @update:project-id="changeProject"
        @update:search="search = $event"
        @select="selectedId = $event"
        @refresh="loadCurrentScope(true)"
        @retry="initialize"
      />

      <MemoryDetailPane
        :memory="selectedMemory"
        :busy="togglingId === selectedMemory?.id"
        :error="actionError"
        @toggle="toggleMemory"
      />
    </div>
  </section>
</template>

<style scoped>
.agent-memory {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  min-height: 0;
  overflow: hidden;
}

.agent-memory__header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
}

.agent-memory__header > div {
  min-width: 0;
}

.memory-workspace {
  display: grid;
  grid-template-columns: minmax(260px, 320px) minmax(0, 1fr);
  gap: 12px;
  min-height: 0;
}

@media (max-width: 760px) {
  .agent-memory {
    overflow: auto;
  }

  .agent-memory__header {
    align-items: flex-start;
    flex-direction: column;
  }

  .memory-workspace {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(260px, 42vh) auto;
  }
}
</style>
