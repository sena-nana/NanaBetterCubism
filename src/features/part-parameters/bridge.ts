import { invoke } from "@tauri-apps/api/core";
import { domainError, isTauriRuntime } from "../parameters/bridge";
import type { PartParameterQueryResult } from "./types";

export async function findSelectedPartParameters(): Promise<PartParameterQueryResult> {
  if (!isTauriRuntime()) {
    throw domainError("desktop_required", "请先在桌面应用中连接 Cubism Editor。");
  }
  return invoke<PartParameterQueryResult>("find_selected_part_parameters");
}
