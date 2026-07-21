import { open } from "@tauri-apps/plugin-dialog";
import type { Ref } from "vue";
import { normalizeCommandError, preparePsd } from "./bridge";
import { MAX_CHAT_PSD } from "./useChatPsdDocuments";
import type { ChatPsdDocument, ChatPsdDraft } from "./types";

function basename(path: string): string {
  const match = path.match(/[^\\/]+$/);
  return match ? match[0] : "document.psd";
}

export function useHomePsdDrafts(options: {
  drafts: Ref<ChatPsdDraft[]>;
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
      if (options.drafts.value.length >= MAX_CHAT_PSD) {
        options.setError(`每个对话最多附加 ${MAX_CHAT_PSD} 个 PSD 文件。`);
        return;
      }
      const draft: ChatPsdDraft = {
        id: crypto.randomUUID(),
        name: basename(path),
        path,
      };
      options.drafts.value = [...options.drafts.value, draft];
    } catch (error) {
      options.setError(normalizeCommandError(error).message);
    }
  }

  function removePsdDraft(psdId: string) {
    options.drafts.value = options.drafts.value.filter((draft) => draft.id !== psdId);
  }

  async function prepareAll(conversationId: string): Promise<ChatPsdDocument[]> {
    const prepared: ChatPsdDocument[] = [];
    for (const draft of options.drafts.value) {
      const result = await preparePsd(conversationId, draft.path);
      prepared.push(result.document);
    }
    return prepared;
  }

  return { pickPsd, removePsdDraft, prepareAll };
}
