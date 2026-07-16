<script setup lang="ts">
import { SettingsRow, UiButton, UiCard, UiInput } from "@lilia/ui";
import { computed, onMounted, ref } from "vue";
import {
  normalizeCommandError,
  setLlmConfig,
  testLlmConnection,
} from "../bridge";
import { useLlmConfigStore } from "../llmConfigStore";
import type { LlmConfigView, LlmTestResult } from "../types";

type Operation = "save" | "test" | "clear";
type Feedback = { text: string; tone: "ok" | "err" };

const baseUrl = ref("");
const apiKey = ref("");
const model = ref("");
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
  hasApiKey.value = config.hasApiKey;
  apiKey.value = "";
}

async function persistConfig(clearApiKey = false) {
  const next = await setLlmConfig({
    baseUrl: baseUrl.value.trim() || null,
    apiKey: clearApiKey ? null : apiKey.value.trim() || null,
    model: model.value.trim() || null,
    clearApiKey,
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
    testResult.value = await testLlmConnection();
    if (testResult.value.ok) {
      availableModels.value = testResult.value.models;
      if (!model.value.trim() && availableModels.value[0]) {
        model.value = availableModels.value[0];
        await persistConfig();
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
    <UiCard title="模型配置" :loading="loading" agent-id="settings.llm.card">
      <p class="card-description">配置 OpenAI 兼容 API。密钥仅保存在本机凭据库，不会回显。</p>

      <template v-if="!loading">
        <SettingsRow label="Base URL" hint="例如 https://api.openai.com/v1 或本地兼容代理。">
          <UiInput
            v-model="baseUrl"
            :disabled="operationBusy"
            placeholder="https://api.openai.com/v1"
            agent-id="settings.llm.base-url"
          />
        </SettingsRow>

        <SettingsRow
          label="API Key"
          :hint="hasApiKey ? '已保存密钥。留空保存可保留原密钥。' : '尚未保存密钥。'"
        >
          <UiInput
            v-model="apiKey"
            :disabled="operationBusy"
            type="password"
            placeholder="sk-..."
            agent-id="settings.llm.api-key"
          />
        </SettingsRow>

        <SettingsRow label="Model" hint="聊天与工具调用使用的模型 ID。可手动填写，或测试连接后从列表选择。">
          <UiInput
            v-model="model"
            :disabled="operationBusy"
            placeholder="gpt-4o-mini"
            agent-id="settings.llm.model"
          />
        </SettingsRow>

        <SettingsRow
          v-if="availableModels.length"
          label="可用模型"
          hint="来自最近一次成功的连接测试。"
          stacked
        >
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
        </SettingsRow>

        <div class="actions">
          <UiButton
            variant="primary"
            :busy="operation === 'save'"
            :disabled="operationBusy"
            agent-id="settings.llm.save"
            @click="save"
          >
            保存
          </UiButton>
          <UiButton
            :busy="operation === 'test'"
            :disabled="operationBusy"
            agent-id="settings.llm.test"
            @click="test"
          >
            测试连接
          </UiButton>
          <UiButton
            v-if="hasApiKey"
            :busy="operation === 'clear'"
            :disabled="operationBusy"
            agent-id="settings.llm.clear-key"
            @click="clearKey"
          >
            清除密钥
          </UiButton>
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
        </p>
      </template>
    </UiCard>
  </section>
</template>

<style scoped>
.card-description {
  margin: -2px 0 8px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}
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
