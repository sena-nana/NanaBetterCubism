<script setup lang="ts">
import { SettingsRow, UiButton, UiInput } from "@lilia/ui";
import { onMounted, ref } from "vue";
import {
  getLlmConfig,
  normalizeCommandError,
  setLlmConfig,
  testLlmConnection,
} from "../bridge";
import type { LlmConfigView, LlmTestResult } from "../types";

const baseUrl = ref("");
const apiKey = ref("");
const model = ref("");
const hasApiKey = ref(false);
const busy = ref(false);
const testBusy = ref(false);
const message = ref<string | null>(null);
const testResult = ref<LlmTestResult | null>(null);
const availableModels = ref<string[]>([]);

onMounted(async () => {
  const config = await getLlmConfig();
  applyConfig(config);
});

function applyConfig(config: LlmConfigView) {
  baseUrl.value = config.baseUrl ?? "";
  model.value = config.model ?? "";
  hasApiKey.value = config.hasApiKey;
  apiKey.value = "";
}

async function save() {
  busy.value = true;
  message.value = null;
  try {
    const next = await setLlmConfig({
      baseUrl: baseUrl.value.trim() || null,
      apiKey: apiKey.value.trim() || null,
      model: model.value.trim() || null,
      clearApiKey: false,
    });
    applyConfig(next);
    message.value = "已保存模型配置。";
  } catch (err) {
    message.value = normalizeCommandError(err).message;
  } finally {
    busy.value = false;
  }
}

async function clearKey() {
  busy.value = true;
  message.value = null;
  try {
    const next = await setLlmConfig({
      baseUrl: baseUrl.value.trim() || null,
      apiKey: null,
      model: model.value.trim() || null,
      clearApiKey: true,
    });
    applyConfig(next);
    message.value = "已清除 API Key。";
  } catch (err) {
    message.value = normalizeCommandError(err).message;
  } finally {
    busy.value = false;
  }
}

async function test() {
  testBusy.value = true;
  testResult.value = null;
  message.value = null;
  try {
    await save();
    testResult.value = await testLlmConnection();
    if (testResult.value.ok) {
      availableModels.value = testResult.value.models;
      if (!model.value.trim() && availableModels.value[0]) {
        model.value = availableModels.value[0];
        await save();
      }
    }
  } catch (err) {
    message.value = normalizeCommandError(err).message;
  } finally {
    testBusy.value = false;
  }
}
</script>

<template>
  <section class="settings-section" data-agent-id="settings.llm">
    <header class="page-header">
      <h1>模型</h1>
      <p>配置 OpenAI 兼容 API。密钥仅保存在本机凭据库，不会回显。</p>
    </header>

    <SettingsRow label="Base URL" description="例如 https://api.openai.com/v1 或本地兼容代理。">
      <UiInput
        v-model="baseUrl"
        placeholder="https://api.openai.com/v1"
        agent-id="settings.llm.base-url"
      />
    </SettingsRow>

    <SettingsRow
      label="API Key"
      :description="hasApiKey ? '已保存密钥。留空保存可保留原密钥。' : '尚未保存密钥。'"
    >
      <UiInput
        v-model="apiKey"
        type="password"
        placeholder="sk-..."
        agent-id="settings.llm.api-key"
      />
    </SettingsRow>

    <SettingsRow label="Model" description="聊天与工具调用使用的模型 ID。可手动填写，或测试连接后从列表选择。">
      <UiInput
        v-model="model"
        placeholder="gpt-4o-mini"
        agent-id="settings.llm.model"
      />
    </SettingsRow>

    <SettingsRow
      v-if="availableModels.length"
      label="可用模型"
      description="来自最近一次成功的 /models 测试。"
    >
      <div class="model-pool" data-agent-id="settings.llm.model-pool">
        <button
          v-for="id in availableModels"
          :key="id"
          type="button"
          class="model-chip"
          :class="{ 'is-active': model === id }"
          :data-agent-id="`settings.llm.model.${id}`"
          @click="model = id"
        >
          {{ id }}
        </button>
      </div>
    </SettingsRow>

    <div class="actions">
      <UiButton variant="primary" :busy="busy" agent-id="settings.llm.save" @click="save">
        保存
      </UiButton>
      <UiButton :busy="testBusy" agent-id="settings.llm.test" @click="test">
        测试连接
      </UiButton>
      <UiButton
        v-if="hasApiKey"
        agent-id="settings.llm.clear-key"
        @click="clearKey"
      >
        清除密钥
      </UiButton>
    </div>

    <p v-if="message" class="message" data-agent-id="settings.llm.message">{{ message }}</p>
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
  </section>
</template>

<style scoped>
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
  border-color: var(--accent, var(--text));
  background: color-mix(in srgb, var(--accent, var(--text)) 12%, transparent);
}
</style>
