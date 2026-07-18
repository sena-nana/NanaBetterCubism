import Brain from "@lucide/vue/dist/esm/icons/brain.mjs";
import Server from "@lucide/vue/dist/esm/icons/server.mjs";
import { markRaw, reactive, type Component } from "vue";
import { SIDEBAR_FOOTER_STATUSES } from "../../ui/shell-state";

export interface FooterPresentation {
  label: string;
  title: string;
  tone: "ok" | "warn" | "error";
  to: string;
  icon: Component;
}

export const modelFooterStatus = reactive<FooterPresentation>({
  label: "模型读取中",
  title: "正在读取模型配置。",
  tone: "warn",
  to: "/settings?tab=model-config",
  icon: markRaw(Brain),
});

export const editorFooterStatus = reactive<FooterPresentation>({
  label: "Editor 未连接",
  title: "在设置中连接 Cubism Editor。",
  tone: "warn",
  to: "/settings?tab=editor",
  icon: markRaw(Server),
});

export function publishModelFooter(presentation: Pick<FooterPresentation, "label" | "title" | "tone">) {
  Object.assign(modelFooterStatus, presentation);
  publishShellFooter("model", presentation);
}

export function publishEditorFooter(presentation: Pick<FooterPresentation, "label" | "title" | "tone">) {
  Object.assign(editorFooterStatus, presentation);
  publishShellFooter("editor", presentation);
}

function publishShellFooter(
  key: string,
  presentation: Pick<FooterPresentation, "label" | "title" | "tone">,
) {
  const status = SIDEBAR_FOOTER_STATUSES.find((item) => item.key === key);
  if (status) Object.assign(status, presentation);
}
