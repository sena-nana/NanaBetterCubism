import { Brain, Server } from "@lucide/vue";
import {
  SIDEBAR_FOOTER_STATUS,
  SIDEBAR_FOOTER_STATUSES,
} from "@lilia/ui";
import type { Component } from "vue";

export interface FooterPresentation {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
  to: string;
  icon: Component;
}

const defaultModel: FooterPresentation = {
  label: "模型读取中",
  title: "正在读取模型配置。",
  tone: "warn",
  to: "/settings?tab=model-config",
  icon: Brain,
};

const defaultEditor: FooterPresentation = {
  label: "Editor 未连接",
  title: "在设置中连接 Cubism Editor。",
  tone: "warn",
  to: "/settings?tab=editor",
  icon: Server,
};

const bothReady: FooterPresentation = {
  label: "就绪",
  title: "模型与 Editor 均可用。",
  tone: "ok",
  to: "/settings?tab=editor",
  icon: Server,
};

let model: FooterPresentation = { ...defaultModel };
let editor: FooterPresentation = { ...defaultEditor };

export function publishModelFooter(presentation: {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
}) {
  model = {
    ...presentation,
    to: "/settings?tab=model-config",
    icon: Brain,
  };
  renderSelfCheckFooter();
}

export function publishEditorFooter(presentation: {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
}) {
  editor = {
    ...presentation,
    to: "/settings?tab=editor",
    icon: Server,
  };
  renderSelfCheckFooter();
}

function renderSelfCheckFooter() {
  const footer =
    SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck") ??
    (SIDEBAR_FOOTER_STATUSES.length === 1 ? SIDEBAR_FOOTER_STATUS : undefined);
  if (!footer) return;

  if (model.tone !== "ok") {
    Object.assign(footer, model);
    return;
  }
  if (editor.tone !== "ok") {
    Object.assign(footer, editor);
    return;
  }
  Object.assign(footer, bothReady);
}
