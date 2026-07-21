import { open } from "@tauri-apps/plugin-dialog";
import type { Ref } from "vue";
import { discardPsd, normalizeCommandError, preparePsd } from "./bridge";
import type { ChatPsdDocument } from "./types";

export function useChatPsdDocuments(options: {
  conversationId: () => string;
  documents: Ref<ChatPsdDocument[]>;
  canInteract: () => boolean;
  setError: (message: string | null) => void;
}) {
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
      const result = await preparePsd(options.conversationId(), path);
      options.documents.value = [...options.documents.value, result.document];
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

  return { pickPsd, removePsd };
}
