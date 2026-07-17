import { reactive } from "vue";
import { getLlmConfig } from "./bridge";
import { publishModelFooter } from "../shell/footerSelfCheck";
import type { LlmConfigView } from "./types";

const defaultConfig: LlmConfigView = {
  baseUrl: null,
  model: null,
  hasApiKey: false,
};

const state = reactive({
  config: defaultConfig,
  initialized: false,
  loading: false,
});

let initializePromise: Promise<LlmConfigView> | null = null;

async function initialize(): Promise<LlmConfigView> {
  if (state.initialized) {
    publishModelFooter(configPresentation(state.config));
    return state.config;
  }
  if (initializePromise) return initializePromise;

  state.loading = true;
  publishModelFooter({
    label: "模型读取中",
    title: "正在读取模型配置。",
    tone: "warn",
  });
  initializePromise = getLlmConfig()
    .then((config) => {
      applyConfig(config);
      return state.config;
    })
    .catch((error) => {
      publishModelFooter({
        label: "模型状态异常",
        title: "无法读取模型配置。点击进入设置。",
        tone: "error",
      });
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
  publishModelFooter(configPresentation(config));
}

export function useLlmConfigStore() {
  return { state, initialize, applyConfig };
}

function configPresentation(config: LlmConfigView): {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
} {
  if (!config.hasApiKey) {
    return {
      label: "模型未配置",
      title: "尚未配置模型 API Key。点击进入设置。",
      tone: "warn",
    };
  }
  const model = config.model?.trim();
  return {
    label: model || "模型已配置",
    title: model ? `已保存模型 ${model}。点击进入设置。` : "已保存模型配置。点击进入设置。",
    tone: "ok",
  };
}
