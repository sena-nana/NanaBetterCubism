import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { UiImageViewerSource } from "@lilia/image-viewer/components/ImageViewer";
import { ref, type Ref } from "vue";
import { discardImageDrafts, normalizeCommandError, prepareImages } from "./bridge";
import { isTauriRuntime } from "../editor/bridge";
import type { ChatImageAttachment, ChatImageDraft, ImagePrepareInput } from "./types";

export const MAX_CHAT_IMAGES = 8;

export function chatImageSrc(image: ChatImageAttachment) {
  if (!image.available) return "";
  return isTauriRuntime() ? convertFileSrc(image.path) : image.path;
}

export function chatImageViewerSource(
  image: ChatImageAttachment,
): UiImageViewerSource | null {
  if (!image.available) return null;
  const size = image.size < 1024 * 1024
    ? `${Math.max(1, Math.round(image.size / 1024))} KiB`
    : `${(image.size / 1024 / 1024).toFixed(1)} MiB`;
  return {
    src: chatImageSrc(image),
    alt: image.name,
    name: image.name,
    metadata: `${image.mime} · ${size}`,
  };
}

export function useChatImageDrafts(options: {
  drafts: Ref<ChatImageDraft[]>;
  canInteract: () => boolean;
  setError: (message: string | null) => void;
}) {
  const viewingImage = ref<UiImageViewerSource | null>(null);

  async function addInputs(inputs: ImagePrepareInput[]): Promise<string[]> {
    if (!options.canInteract() || inputs.length === 0) return [];
    try {
      const result = await prepareImages(
        inputs,
        Math.max(0, MAX_CHAT_IMAGES - options.drafts.value.length),
      );
      options.drafts.value = [...options.drafts.value, ...result.accepted];
      return result.rejected.map((item) => `${item.name}：${item.message}`);
    } catch (error) {
      return [normalizeCommandError(error).message];
    }
  }

  async function addPaths(paths: string[]): Promise<string[]> {
    return addInputs(paths.map((path) => ({ kind: "path", path })));
  }

  async function pickImages() {
    if (!options.canInteract()) return;
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
      if (!selected) return;
      const errors = await addPaths(Array.isArray(selected) ? selected : [selected]);
      options.setError(errors.join("\n") || null);
    } catch (error) {
      options.setError(normalizeCommandError(error).message);
    }
  }

  async function pasteImages(event: ClipboardEvent) {
    if (!options.canInteract()) return;
    const files = Array.from(event.clipboardData?.items ?? [])
      .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
      .map((item) => item.getAsFile())
      .filter((file): file is File => Boolean(file));
    if (files.length === 0) return;
    event.preventDefault();
    const inputs = await Promise.all(
      files.map(async (file): Promise<ImagePrepareInput> => ({
        kind: "bytes",
        name: file.name || "clipboard-image",
        bytesBase64: bytesToBase64(new Uint8Array(await file.arrayBuffer())),
      })),
    );
    options.setError((await addInputs(inputs)).join("\n") || null);
  }

  async function removeImage(draftId: string) {
    const existing = options.drafts.value.find((draft) => draft.draftId === draftId);
    if (!existing) return;
    options.drafts.value = options.drafts.value.filter((draft) => draft.draftId !== draftId);
    try {
      await discardImageDrafts([draftId]);
    } catch (error) {
      options.drafts.value = [...options.drafts.value, existing];
      options.setError(normalizeCommandError(error).message);
    }
  }

  function viewImage(image: ChatImageAttachment) {
    viewingImage.value = chatImageViewerSource(image);
  }

  return { addPaths, pickImages, pasteImages, removeImage, viewImage, viewingImage };
}

function bytesToBase64(bytes: Uint8Array) {
  let binary = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}
