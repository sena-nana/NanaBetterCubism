import {
  SIDEBAR_FOOTER_STATUS,
  SIDEBAR_FOOTER_STATUSES,
  type SidebarFooterStatus,
} from "@lilia/ui";
import { reactive } from "vue";
import { getLlmConfig } from "./bridge";
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
    updateModelFooter(configPresentation(state.config));
    return state.config;
  }
  if (initializePromise) return initializePromise;

  state.loading = true;
  updateModelFooter({
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
      updateModelFooter({
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
  updateModelFooter(configPresentation(config));
}

export function useLlmConfigStore() {
  return { state, initialize, applyConfig };
}

function configPresentation(config: LlmConfigView): FooterPresentation {
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

function updateModelFooter(presentation: FooterPresentation) {
  const footer = footerStatus("model");
  if (!footer) return;
  Object.assign(footer, {
    to: "/settings?tab=model-config",
    ...presentation,
  });
}

function footerStatus(key: string): SidebarFooterStatus | undefined {
  return SIDEBAR_FOOTER_STATUSES.find((status) => status.key === key)
    ?? (SIDEBAR_FOOTER_STATUSES.length === 1 ? SIDEBAR_FOOTER_STATUS : undefined);
}

interface FooterPresentation {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
}
