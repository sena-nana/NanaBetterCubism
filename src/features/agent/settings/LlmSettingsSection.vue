<script setup lang="ts">
import { Button, Card, Input, Select } from "../../../ui";
import { computed, onMounted, ref } from "vue";
import { normalizeCommandError, setLlmConfig } from "../bridge";
import { useLlmConfigStore } from "../llmConfigStore";
import type { LlmConfigView, LlmTestResult } from "../types";

type Operation = "save" | "api-test" | "model-test" | "clear";
type Feedback = { text: string; tone: "ok" | "err" };

const baseUrl = ref("");
const apiKey = ref("");
const selectedModel = ref("");
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
const modelOptions = computed(() =>
  availableModels.value.map((id) => ({ label: id, value: id })),
);

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
  selectedModel.value = config.model ?? "";
  contextWindow.value = config.contextWindow ? String(config.contextWindow) : "";
  maxInputTokens.value = config.maxInputTokens ? String(config.maxInputTokens) : "";
  hasApiKey.value = config.hasApiKey;
  apiKey.value = "";
}

function updateEndpoint(field: "baseUrl" | "apiKey", value: string) {
  if (field === "baseUrl") baseUrl.value = value;
  else apiKey.value = value;
  selectedModel.value = "";
  resetResult();
  llmConfig.invalidateConnection();
}

function selectModel(value: string | number) {
  selectedModel.value = String(value);
  testResult.value = null;
}

function parsePositiveInt(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0) return null;
  return parsed;
}

async function persistConfig(options: { clearApiKey?: boolean; model?: string | null } = {}) {
  const clearApiKey = options.clearApiKey ?? false;
  const next = await setLlmConfig({
    baseUrl: baseUrl.value.trim() || null,
    apiKey: clearApiKey ? null : apiKey.value.trim() || null,
    model: options.model === undefined
      ? llmConfig.state.config.model
      : options.model?.trim() || null,
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
    await persistConfig({
      clearApiKey: kind === "clear",
      model: kind === "clear" ? null : undefined,
    });
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

async function testApiConnection() {
  operation.value = "api-test";
  resetResult();
  const previousModel = selectedModel.value || llmConfig.state.config.model || "";
  try {
    await persistConfig({ model: null });
    const result = await llmConfig.discoverModels();
    if (!result) return;
    availableModels.value = result.models;
    selectedModel.value = result.models.includes(previousModel) ? previousModel : "";
    feedback.value = result.models.length
      ? { text: `API 连接成功，发现 ${result.models.length} 个可用模型。`, tone: "ok" }
      : { text: "API 已连接，但没有返回可用模型。", tone: "err" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

async function testSelectedModel() {
  const model = selectedModel.value.trim();
  if (!model) return;
  operation.value = "model-test";
  feedback.value = null;
  testResult.value = null;
  try {
    testResult.value = await llmConfig.testModel(model);
    if (testResult.value.ok && testResult.value.config) {
      applyForm(testResult.value.config);
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
            :model-value="baseUrl"
            :disabled="operationBusy"
            placeholder="https://api.openai.com/v1"
            agent-id="settings.llm.base-url"
            @update:model-value="updateEndpoint('baseUrl', $event)"
          />
        </label>

        <label class="settings-field">
          <span><strong>API Key</strong><small>{{ hasApiKey ? "已保存密钥。留空保存可保留原密钥。" : "尚未保存密钥。" }}</small></span>
          <Input
            :model-value="apiKey"
            :disabled="operationBusy"
            type="password"
            placeholder="sk-..."
            agent-id="settings.llm.api-key"
            @update:model-value="updateEndpoint('apiKey', $event)"
          />
        </label>

        <label class="settings-field">
          <span><strong>上下文窗口</strong><small>模型上下文窗口大小（token）。留空则按默认 256000 估算。</small></span>
          <Input
            v-model="contextWindow"
            :disabled="operationBusy"
            placeholder="256000"
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

        <label v-if="availableModels.length" class="settings-field">
          <span><strong>模型</strong><small>选择模型后，再执行模型连接测试。</small></span>
          <Select
            :model-value="selectedModel"
            :options="modelOptions"
            :disabled="operationBusy"
            placeholder="请选择模型"
            aria-label="模型"
            agent-id="settings.llm.model-select"
            @update:model-value="selectModel"
          />
        </label>

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
            :loading="operation === 'api-test'"
            :disabled="operationBusy"
            agent-id="settings.llm.api-test"
            @click="testApiConnection"
          >
            测试 API 连接
          </Button>
          <Button
            v-if="availableModels.length"
            variant="primary"
            :loading="operation === 'model-test'"
            :disabled="operationBusy || !selectedModel"
            agent-id="settings.llm.model-test"
            @click="testSelectedModel"
          >
            测试模型连接
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
</style>
