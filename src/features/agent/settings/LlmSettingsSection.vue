<script setup lang="ts">
import { Button, Card, Input } from "../../../ui";
import { computed, onMounted, ref } from "vue";
import { normalizeCommandError, setLlmConfig } from "../bridge";
import { useLlmConfigStore } from "../llmConfigStore";
import type { LlmConfigView, LlmTestResult } from "../types";

type Operation = "save" | "test" | "clear";
type Feedback = { text: string; tone: "ok" | "err" };

const baseUrl = ref("");
const apiKey = ref("");
const model = ref("");
const contextWindow = ref("");
const maxInputTokens = ref("");
const hasApiKey = ref(false);
const loading = ref(true);
const operation = ref<Operation | null>(null);
const feedback = ref<Feedback | null>(null);
const testResult = ref<LlmTestResult | null>(null);
const availableModels = ref<string[]>([]);
const llmConfig = useLlmConfigStore();
const operationBusy = computed(() => loading.value || operation.value !== null);

onMounted(async () => {
  try {
    applyForm(await llmConfig.initialize());
  } catch (err) {
    setError(err);
  } finally {
    loading.value = false;
  }
});

function applyForm(config: LlmConfigView) {
  baseUrl.value = config.baseUrl ?? "";
  model.value = config.model ?? "";
  contextWindow.value = config.contextWindow ? String(config.contextWindow) : "";
  maxInputTokens.value = config.maxInputTokens ? String(config.maxInputTokens) : "";
  hasApiKey.value = config.hasApiKey;
  apiKey.value = "";
}

function parsePositiveInt(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0) return null;
  return parsed;
}

async function persistConfig(clearApiKey = false) {
  const next = await setLlmConfig({
    baseUrl: baseUrl.value.trim() || null,
    apiKey: clearApiKey ? null : apiKey.value.trim() || null,
    model: model.value.trim() || null,
    clearApiKey,
    contextWindow: parsePositiveInt(contextWindow.value),
    maxInputTokens: parsePositiveInt(maxInputTokens.value),
  });
  llmConfig.applyConfig(next);
  applyForm(next);
}

function resetResult() {
  feedback.value = null;
  testResult.value = null;
  availableModels.value = [];
}

function setError(error: unknown) {
  feedback.value = { text: normalizeCommandError(error).message, tone: "err" };
}

async function updateConfig(kind: "save" | "clear", success: string) {
  operation.value = kind;
  resetResult();
  try {
    await persistConfig(kind === "clear");
    feedback.value = { text: success, tone: "ok" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

function save() {
  return updateConfig("save", "已保存模型配置。");
}

function clearKey() {
  return updateConfig("clear", "已清除 API Key。");
}

async function test() {
  operation.value = "test";
  resetResult();
  try {
    await persistConfig();
    testResult.value = await llmConfig.testConnection();
    if (!testResult.value.ok) return;
    availableModels.value = testResult.value.models;
    if (!model.value.trim() && testResult.value.models[0]) {
      model.value = testResult.value.models[0];
      await persistConfig();
      testResult.value = await llmConfig.testConnection();
      if (testResult.value.models.length) {
        availableModels.value = testResult.value.models;
      }
    }
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}
</script>

<template>
  <section class="settings-section" data-agent-id="settings.llm">
    <Card :loading="loading" agent-id="settings.llm.card">
      <h2>模型配置</h2>
      <p class="card-description">配置 OpenAI 兼容 API。密钥仅保存在本机凭据库，不会回显。</p>

      <template v-if="!loading">
        <label class="settings-field">
          <span><strong>Base URL</strong><small>例如 https://api.openai.com/v1 或本地兼容代理。</small></span>
          <Input
            v-model="baseUrl"
            :disabled="operationBusy"
            placeholder="https://api.openai.com/v1"
            agent-id="settings.llm.base-url"
          />
        </label>

        <label class="settings-field">
          <span><strong>API Key</strong><small>{{ hasApiKey ? "已保存密钥。留空保存可保留原密钥。" : "尚未保存密钥。" }}</small></span>
          <Input
            v-model="apiKey"
            :disabled="operationBusy"
            type="password"
            placeholder="sk-..."
            agent-id="settings.llm.api-key"
          />
        </label>

        <label class="settings-field">
          <span><strong>Model</strong><small>聊天与工具调用使用的模型 ID。可手动填写，或测试连接后从列表选择。</small></span>
          <Input
            v-model="model"
            :disabled="operationBusy"
            placeholder="gpt-4o-mini"
            agent-id="settings.llm.model"
          />
        </label>

        <label class="settings-field">
          <span><strong>上下文窗口</strong><small>模型上下文窗口大小（token）。留空则不启用自动压缩。</small></span>
          <Input
            v-model="contextWindow"
            :disabled="operationBusy"
            placeholder="128000"
            agent-id="settings.llm.context-window"
          />
        </label>

        <label class="settings-field">
          <span><strong>输入预算</strong><small>每次请求输入 token 上限。留空则按上下文窗口的 70% 估算。</small></span>
          <Input
            v-model="maxInputTokens"
            :disabled="operationBusy"
            placeholder="100000"
            agent-id="settings.llm.max-input-tokens"
          />
        </label>

        <div
          v-if="availableModels.length"
          class="settings-field settings-field--stacked"
        >
          <span><strong>可用模型</strong><small>来自最近一次成功的连接测试。</small></span>
          <div class="model-pool" data-agent-id="settings.llm.model-pool">
            <button
              v-for="id in availableModels"
              :key="id"
              type="button"
              class="model-chip"
              :class="{ 'is-active': model === id }"
              :disabled="operationBusy"
              :data-agent-id="`settings.llm.model.${id}`"
              @click="model = id"
            >
              {{ id }}
            </button>
          </div>
        </div>

        <div class="actions">
          <Button
            variant="primary"
            :loading="operation === 'save'"
            :disabled="operationBusy"
            agent-id="settings.llm.save"
            @click="save"
          >
            保存
          </Button>
          <Button
            :loading="operation === 'test'"
            :disabled="operationBusy"
            agent-id="settings.llm.test"
            @click="test"
          >
            测试连接
          </Button>
          <Button
            v-if="hasApiKey"
            :loading="operation === 'clear'"
            :disabled="operationBusy"
            agent-id="settings.llm.clear-key"
            @click="clearKey"
          >
            清除密钥
          </Button>
        </div>

        <p
          v-if="feedback"
          class="message"
          :class="`message--${feedback.tone}`"
          :role="feedback.tone === 'err' ? 'alert' : undefined"
          data-agent-id="settings.llm.message"
        >
          {{ feedback.text }}
        </p>
        <p
          v-if="testResult"
          class="message"
          :class="testResult.ok ? 'message--ok' : 'message--err'"
          data-agent-id="settings.llm.test-result"
        >
          {{ testResult.message }}
          <template v-if="testResult.models.length">
            （可用模型 {{ testResult.models.length }} 个）
          </template>
          <template v-if="testResult.imageSupported === false">
            ；该模型不支持图片输入，「查看 Editor 窗口」等能力将禁用，请更换支持视觉的模型。
          </template>
        </p>
      </template>
    </Card>
  </section>
</template>

<style scoped>
.card-description {
  margin: -2px 0 8px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}
.settings-field { display: grid; grid-template-columns: minmax(150px, .6fr) minmax(220px, 1fr); align-items: center; gap: 16px; padding: 10px 0; }
.settings-field > span { display: grid; gap: 3px; }
.settings-field strong { font-size: 12px; }
.settings-field small { color: var(--text-muted); line-height: 1.4; }
.settings-field--stacked { grid-template-columns: 1fr; }
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.message {
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-muted);
}
.message--ok {
  color: var(--ok);
}
.message--err {
  color: var(--err);
}
.model-pool {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-height: 160px;
  overflow: auto;
}
.model-chip {
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text);
  padding: 4px 8px;
  font-size: 12px;
  border-radius: 4px;
  cursor: pointer;
}
.model-chip.is-active {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent);
}
.model-chip:disabled {
  cursor: default;
  opacity: 0.6;
}
</style>
