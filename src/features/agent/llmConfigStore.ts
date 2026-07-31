import { reactive } from "vue";
import {
  getLlmConfig,
  listLlmModels,
  listenImageCapability,
  testLlmConnection,
  testLlmModel,
} from "./bridge";
import { publishModelFooter } from "../shell/footerSelfCheck";
import type { LlmConfigView, LlmModelListResult, LlmTestResult } from "./types";

export type LlmConnectionStatus =
  | "unconfigured"
  | "checking"
  | "model_required"
  | "ready"
  | "stale"
  | "failed";

interface LlmConfigState {
  config: LlmConfigView;
  initialized: boolean;
  loading: boolean;
  connectionStatus: LlmConnectionStatus;
  imageInputSupported: boolean | null;
}

const defaultConfig: LlmConfigView = {
  apiMode: "chat_completions",
  baseUrl: null,
  model: null,
  hasApiKey: false,
  contextWindow: null,
  maxInputTokens: null,
};

const state = reactive<LlmConfigState>({
  config: defaultConfig,
  initialized: false,
  loading: false,
  connectionStatus: "unconfigured",
  imageInputSupported: null,
});

let initializePromise: Promise<LlmConfigView> | null = null;
let checkToken = 0;
let capabilityInstalled = false;

async function installImageCapabilityListener() {
  if (capabilityInstalled) return;
  capabilityInstalled = true;
  await listenImageCapability((payload) => {
    state.imageInputSupported = payload.supported ? true : payload.unsupported ? false : null;
  });
}

async function initialize(): Promise<LlmConfigView> {
  if (state.initialized) return state.config;
  if (initializePromise) return initializePromise;

  state.loading = true;
  publishModelFooter({
    label: "模型读取中",
    title: "正在读取模型配置。",
    tone: "warn",
  });
  initializePromise = getLlmConfig()
    .then(async (config) => {
      applyConfig(config);
      void installImageCapabilityListener();
      if (hasCompleteConfig(config)) {
        await testConnection().catch(() => undefined);
      }
      return state.config;
    })
    .catch((error) => {
      publishFailed("模型状态异常", "无法读取模型配置。点击进入设置。");
      throw error;
    })
    .finally(() => {
      state.loading = false;
      initializePromise = null;
    });
  return initializePromise;
}

function applyConfig(config: LlmConfigView) {
  state.config = { ...config };
  state.initialized = true;
  state.imageInputSupported = config.imageInputSupported ?? null;
  checkToken += 1;
  publishConfiguredState();
}

async function testConnection(): Promise<LlmTestResult> {
  const token = ++checkToken;
  state.imageInputSupported = null;
  publishChecking("正在验证已保存的模型连接。");
  try {
    const result = await testLlmConnection();
    if (token === checkToken) applyCheckResult(result);
    return result;
  } catch (error) {
    if (token === checkToken) {
      if (hasCompleteConfig(state.config)) {
        publishFailed("模型连接异常", "最近一次模型连接测试失败。点击进入设置重试。");
      } else {
        publishUnconfigured();
      }
    }
    throw error;
  }
}

async function discoverModels(): Promise<LlmModelListResult | null> {
  const token = ++checkToken;
  state.imageInputSupported = null;
  publishChecking("正在验证 API 并读取模型列表。");
  try {
    const result = await listLlmModels();
    if (token !== checkToken) return null;
    publishModelRequired(result.models.length);
    return result;
  } catch (error) {
    if (token === checkToken) {
      publishFailed("API 连接异常", "无法获取模型列表。点击进入设置重试。");
    }
    throw error;
  }
}

async function testModel(model: string): Promise<LlmTestResult> {
  const token = ++checkToken;
  state.imageInputSupported = null;
  publishChecking("正在验证所选模型连接。");
  try {
    const result = await testLlmModel(model);
    if (token === checkToken) {
      if (result.ok && result.config) {
        state.config = { ...result.config };
        state.imageInputSupported = result.imageSupported ?? null;
        publishReady();
      } else {
        publishFailed("模型连接异常", "所选模型未通过连接测试。点击进入设置重试。");
      }
    }
    return result;
  } catch (error) {
    if (token === checkToken) {
      publishFailed("模型连接异常", "所选模型连接测试失败。点击进入设置重试。");
    }
    throw error;
  }
}

function invalidateConnection() {
  checkToken += 1;
  state.imageInputSupported = null;
  publishConfiguredState();
}

export function useLlmConfigStore() {
  return {
    state,
    initialize,
    applyConfig,
    testConnection,
    discoverModels,
    testModel,
    invalidateConnection,
  };
}

function applyCheckResult(result: LlmTestResult) {
  if (result.imageSupported !== undefined && result.imageSupported !== null) {
    state.imageInputSupported = result.imageSupported;
  }
  if (!result.ok) {
    publishFailed("模型连接异常", "最近一次模型连接测试失败。点击进入设置重试。");
  } else if (hasCompleteConfig(state.config)) {
    publishReady();
  } else {
    publishUnconfigured();
  }
}

function hasCompleteConfig(config: LlmConfigView) {
  return Boolean(config.hasApiKey && config.baseUrl?.trim() && config.model?.trim());
}

function hasEndpointConfig(config: LlmConfigView) {
  return Boolean(config.hasApiKey && config.baseUrl?.trim());
}

function publishConfiguredState() {
  if (hasCompleteConfig(state.config)) publishStale();
  else if (hasEndpointConfig(state.config)) publishModelRequired();
  else publishUnconfigured();
}

function publishUnconfigured() {
  setConnection("unconfigured", {
    label: "模型未配置",
    title: "模型配置不完整。点击进入设置。",
    tone: "warn",
  });
}

function publishChecking(title: string) {
  setConnection("checking", {
    label: "模型检查中",
    title,
    tone: "warn",
  });
}

function publishModelRequired(modelCount?: number) {
  const empty = modelCount === 0;
  setConnection("model_required", {
    label: empty ? "无可用模型" : "待选择模型",
    title: empty
      ? "API 已连接，但没有返回可选模型。点击进入设置。"
      : "API 已配置，请选择模型并测试连接。",
    tone: "warn",
  });
}

function publishStale() {
  setConnection("stale", {
    label: "模型待检查",
    title: "模型配置已更新，需要重新测试连接。",
    tone: "warn",
  });
}

function publishFailed(label: string, title: string) {
  setConnection("failed", { label, title, tone: "error" });
}

function publishReady() {
  const model = state.config.model!.trim();
  setConnection("ready", {
    label: model,
    title: `模型 ${model} 已通过本次连接测试。点击进入设置。`,
    tone: "ok",
  });
}

function setConnection(
  status: LlmConnectionStatus,
  footer: { label: string; title: string; tone: "ok" | "warn" | "error" },
) {
  state.connectionStatus = status;
  publishModelFooter(footer);
}
