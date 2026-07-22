import { open } from "@tauri-apps/plugin-dialog";
import type { Ref } from "vue";
import { discardPsd, normalizeCommandError, preparePsd } from "./bridge";
import type { ChatPsdDocument } from "./types";

export const MAX_CHAT_PSD = 8;

function basename(path: string): string {
  return path.match(/[^\\/]+$/)?.[0] ?? "document.psd";
}

export function useChatPsdDocuments(options: {
  conversationId: () => string;
  documents: Ref<ChatPsdDocument[]>;
  canInteract: () => boolean;
  setError: (message: string | null) => void;
}) {
  async function addPaths(paths: string[]): Promise<string[]> {
    if (!options.canInteract() || paths.length === 0) return [];
    const errors: string[] = [];
    let limitReached = false;
    for (const path of paths) {
      if (options.documents.value.length >= MAX_CHAT_PSD) {
        limitReached = true;
        continue;
      }
      try {
        const result = await preparePsd(options.conversationId(), path);
        options.documents.value = [...options.documents.value, result.document];
      } catch (error) {
        errors.push(`${basename(path)}：${normalizeCommandError(error).message}`);
      }
    }
    if (limitReached) errors.push(`每个对话最多附加 ${MAX_CHAT_PSD} 个 PSD 文件。`);
    return errors;
  }

  async function pickPsd() {
    if (!options.canInteract()) return;
    options.setError(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Photoshop", extensions: ["psd"] }],
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      options.setError((await addPaths([path])).join("\n") || null);
    } catch (error) {
      options.setError(normalizeCommandError(error).message);
    }
  }

  async function removePsd(psdId: string) {
    const existing = options.documents.value.find((doc) => doc.id === psdId);
    if (!existing) return;
    const previous = options.documents.value;
    options.documents.value = previous.filter((doc) => doc.id !== psdId);
    try {
      options.documents.value = await discardPsd(options.conversationId(), psdId);
    } catch (error) {
      options.documents.value = previous;
      options.setError(normalizeCommandError(error).message);
    }
  }

  return { addPaths, pickPsd, removePsd };
}
