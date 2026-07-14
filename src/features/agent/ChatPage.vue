<script setup lang="ts">
import { UiButton, UiCard, UiInput, UiSwitch } from "@lilia/ui";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useEditorStore } from "../editor/editorStore";
import {
  answerAsk,
  bindProject,
  cancelTurn,
  consolidateMemory,
  getLlmConfig,
  getMessages,
  getPendingAsk,
  getPlan,
  listProjects,
  listenAsk,
  listenPlan,
  listenToolEvent,
  listenTurnDelta,
  listenTurnFinished,
  normalizeCommandError,
  sendMessage,
  upsertProject,
} from "./bridge";
import type {
  ChatMessage,
  ConversationPlan,
  LlmConfigView,
  PendingAsk,
  ProjectRecord,
} from "./types";
import { ensureSidebarConversationsLoaded } from "./sidebarConversations";

const route = useRoute();
const router = useRouter();
const editor = useEditorStore();

const conversationId = computed(() => String(route.params.id ?? ""));
const messages = ref<ChatMessage[]>([]);
const plan = ref<ConversationPlan | null>(null);
const pendingAsk = ref<PendingAsk | null>(null);
const draft = ref("");
const askAnswer = ref("");
const busy = ref(false);
const consolidating = ref(false);
const error = ref<string | null>(null);
const llm = ref<LlmConfigView>({ baseUrl: null, model: null, hasApiKey: false });
const projects = ref<ProjectRecord[]>([]);
const projectName = ref("");
const selectedProjectId = ref<string>("");
const planOpen = ref(true);

const unlisteners: Array<() => void> = [];

const canSend = computed(
  () => Boolean(draft.value.trim()) && !busy.value && !pendingAsk.value && llm.value.hasApiKey,
);

onMounted(async () => {
  void editor.initialize();
  llm.value = await getLlmConfig();
  projects.value = await listProjects();
  await reloadConversation();
  unlisteners.push(
    await listenTurnDelta(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      appendDelta(payload.text);
    }),
    await listenToolEvent(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      if (payload.status === "started") return;
      messages.value.push({
        id: `tool-${Date.now()}-${payload.toolName}`,
        role: "tool",
        content: payload.summary,
        toolName: payload.toolName,
        toolStatus: payload.status,
        createdAt: new Date().toISOString(),
      });
    }),
    await listenTurnFinished(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      busy.value = false;
      if (!payload.ok) error.value = payload.message;
      await reloadConversation();
      await ensureSidebarConversationsLoaded(true);
    }),
    await listenAsk(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      pendingAsk.value = payload.ask;
      busy.value = false;
    }),
    await listenPlan(async (payload) => {
      if (payload.conversationId !== conversationId.value) return;
      plan.value = payload.plan;
    }),
  );
});

onUnmounted(() => {
  for (const stop of unlisteners) stop();
});

watch(conversationId, () => {
  void reloadConversation();
});

async function reloadConversation() {
  if (!conversationId.value) return;
  error.value = null;
  messages.value = await getMessages(conversationId.value);
  plan.value = await getPlan(conversationId.value);
  pendingAsk.value = await getPendingAsk(conversationId.value);
  const rows = await listProjects();
  projects.value = rows;
}

function appendDelta(text: string) {
  const last = messages.value[messages.value.length - 1];
  if (last?.role === "assistant" && last.id.startsWith("stream-")) {
    last.content += text;
    return;
  }
  messages.value.push({
    id: `stream-${Date.now()}`,
    role: "assistant",
    content: text,
    toolName: null,
    toolStatus: null,
    createdAt: new Date().toISOString(),
  });
}

async function onSend() {
  const content = draft.value.trim();
  if (!content || !canSend.value) return;
  draft.value = "";
  busy.value = true;
  error.value = null;
  messages.value.push({
    id: `local-${Date.now()}`,
    role: "user",
    content,
    toolName: null,
    toolStatus: null,
    createdAt: new Date().toISOString(),
  });
  try {
    await sendMessage(conversationId.value, content);
  } catch (err) {
    busy.value = false;
    error.value = normalizeCommandError(err).message;
  }
}

async function onCancel() {
  await cancelTurn(conversationId.value);
  busy.value = false;
  pendingAsk.value = null;
}

async function onAnswerAsk(answer?: string) {
  if (!pendingAsk.value) return;
  const value = (answer ?? askAnswer.value).trim();
  if (!value) return;
  busy.value = true;
  error.value = null;
  try {
    await answerAsk(pendingAsk.value.askId, value);
    pendingAsk.value = null;
    askAnswer.value = "";
  } catch (err) {
    busy.value = false;
    error.value = normalizeCommandError(err).message;
  }
}

async function onBindProject() {
  const name = projectName.value.trim();
  try {
    if (selectedProjectId.value) {
      await bindProject(conversationId.value, selectedProjectId.value);
    } else if (name) {
      const project = await upsertProject(name);
      await bindProject(conversationId.value, project.id);
      projects.value = await listProjects();
      selectedProjectId.value = project.id;
      projectName.value = "";
    } else {
      await bindProject(conversationId.value, null);
    }
    await ensureSidebarConversationsLoaded(true);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  }
}

async function onConsolidate() {
  consolidating.value = true;
  error.value = null;
  try {
    await consolidateMemory(conversationId.value);
  } catch (err) {
    error.value = normalizeCommandError(err).message;
  } finally {
    consolidating.value = false;
  }
}

function goSettings(tab: string) {
  void router.push(`/settings?tab=${tab}`);
}

function statusLabel(status: string | null) {
  if (status === "started") return "进行中";
  if (status === "finished") return "完成";
  if (status === "failed") return "失败";
  return status ?? "";
}
</script>

<template>
  <section class="page agent-chat" data-agent-id="agent.chat">
    <header class="page-header agent-chat__header">
      <div>
        <h1>对话</h1>
        <p>
          Editor {{ editor.state.snapshot.state }}
          ·
          <button class="linkish" type="button" data-agent-id="agent.chat.open-llm" @click="goSettings('llm')">
            {{ llm.hasApiKey ? (llm.model || "已配置模型") : "未配置模型" }}
          </button>
        </p>
      </div>
      <div class="agent-chat__actions">
        <UiButton
          :busy="consolidating"
          agent-id="agent.chat.consolidate"
          @click="onConsolidate"
        >
          整理记忆
        </UiButton>
        <UiButton
          v-if="busy || pendingAsk"
          agent-id="agent.chat.cancel"
          @click="onCancel"
        >
          停止
        </UiButton>
      </div>
    </header>

    <UiCard title="项目绑定" agent-id="agent.chat.project" class="agent-chat__project">
      <div class="project-row">
        <select
          v-model="selectedProjectId"
          data-agent-id="agent.chat.project-select"
        >
          <option value="">未绑定</option>
          <option v-for="project in projects" :key="project.id" :value="project.id">
            {{ project.name }}
          </option>
        </select>
        <UiInput
          v-model="projectName"
          placeholder="新建项目名"
          agent-id="agent.chat.project-name"
        />
        <UiButton agent-id="agent.chat.project-bind" @click="onBindProject">绑定</UiButton>
      </div>
    </UiCard>

    <UiCard
      v-if="plan && plan.steps.length"
      :title="planOpen ? '计划' : '计划（已折叠）'"
      agent-id="agent.chat.plan"
      class="agent-chat__plan"
    >
      <div class="plan-toolbar">
        <UiSwitch
          :model-value="planOpen"
          label="展开计划"
          agent-id="agent.chat.plan-toggle"
          @update:model-value="(v) => (planOpen = Boolean(v))"
        />
      </div>
      <ol v-if="planOpen" class="plan-list">
        <li v-for="step in plan.steps" :key="step.id" :data-status="step.status">
          <span class="plan-status">{{ step.status }}</span>
          <span>{{ step.title }}</span>
        </li>
      </ol>
    </UiCard>

    <div class="agent-chat__transcript" data-agent-id="agent.chat.transcript">
      <article
        v-for="message in messages"
        :key="message.id"
        class="bubble"
        :class="`bubble--${message.role}`"
        :data-agent-id="`agent.chat.message.${message.id}`"
      >
        <header v-if="message.role === 'tool'" class="bubble__meta">
          {{ message.toolName }} · {{ statusLabel(message.toolStatus) }}
        </header>
        <p class="bubble__body">{{ message.content }}</p>
      </article>
      <p v-if="!messages.length" class="agent-chat__empty">发送消息开始与 Cubism Agent 对话。</p>
    </div>

    <UiCard
      v-if="pendingAsk"
      title="Agent 提问"
      agent-id="agent.chat.ask"
      class="agent-chat__ask"
    >
      <p class="ask-question">{{ pendingAsk.question }}</p>
      <div v-if="pendingAsk.options.length" class="ask-options">
        <UiButton
          v-for="option in pendingAsk.options"
          :key="option"
          :agent-id="`agent.chat.ask-option.${option}`"
          @click="onAnswerAsk(option)"
        >
          {{ option }}
        </UiButton>
      </div>
      <div class="ask-row">
        <UiInput
          v-model="askAnswer"
          placeholder="输入回答"
          agent-id="agent.chat.ask-input"
          @keydown.enter="onAnswerAsk()"
        />
        <UiButton variant="primary" agent-id="agent.chat.ask-submit" @click="onAnswerAsk()">
          回答
        </UiButton>
      </div>
    </UiCard>

    <div v-else class="agent-chat__composer" data-agent-id="agent.chat.composer">
      <UiInput
        v-model="draft"
        placeholder="描述你想在 Cubism Editor 中完成的事…"
        :disabled="busy || !llm.hasApiKey"
        agent-id="agent.chat.input"
        @keydown.enter="onSend"
      />
      <UiButton
        variant="primary"
        :disabled="!canSend"
        :busy="busy"
        agent-id="agent.chat.send"
        @click="onSend"
      >
        发送
      </UiButton>
    </div>

    <p v-if="!llm.hasApiKey" class="agent-chat__hint">
      请先在
      <button class="linkish" type="button" @click="goSettings('llm')">设置 · 模型</button>
      配置 OpenAI 兼容 API。
    </p>
    <p v-if="error" class="agent-chat__error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.agent-chat {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
  height: 100%;
}
.agent-chat__header {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}
.agent-chat__actions {
  display: flex;
  gap: 8px;
}
.project-row,
.ask-row,
.agent-chat__composer {
  display: flex;
  gap: 8px;
  align-items: center;
}
.project-row select,
.agent-chat__composer :deep(input),
.ask-row :deep(input),
.project-row :deep(input) {
  flex: 1;
}
.agent-chat__transcript {
  flex: 1;
  min-height: 240px;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 4px 0;
}
.bubble {
  max-width: 860px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elev);
}
.bubble--user {
  align-self: flex-end;
  background: var(--accent-soft);
}
.bubble--tool {
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 12px;
}
.bubble__meta {
  margin-bottom: 4px;
  font-size: 11px;
  color: var(--text-faint);
}
.bubble__body {
  margin: 0;
  white-space: pre-wrap;
  font-size: 13px;
  line-height: 1.5;
}
.agent-chat__empty,
.agent-chat__hint {
  color: var(--text-muted);
  font-size: 13px;
}
.agent-chat__error {
  color: var(--err);
  font-size: 12px;
  margin: 0;
}
.ask-question {
  margin: 0 0 8px;
  font-size: 13px;
}
.ask-options {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 8px;
}
.plan-list {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
}
.plan-status {
  display: inline-block;
  min-width: 88px;
  color: var(--text-faint);
  font-size: 11px;
}
.linkish {
  border: 0;
  background: transparent;
  color: var(--accent);
  cursor: pointer;
  padding: 0;
  font: inherit;
}
.plan-toolbar {
  margin-bottom: 8px;
}
</style>
