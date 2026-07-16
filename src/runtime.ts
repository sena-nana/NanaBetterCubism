import type { App } from "vue";
import {
  installCornerStyle,
  installGlobalScrollbarVisibility,
  installLiliaContextMenu,
  installNativeAppearance,
} from "@lilia/ui/runtime";

export function installNanaBetterCubismUiRuntime(app: App) {
  installLiliaContextMenu(app);
  installGlobalScrollbarVisibility();
  installCornerStyle();
  installNativeAppearance();
}
