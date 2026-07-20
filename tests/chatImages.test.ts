import { fireEvent, render, screen } from "@testing-library/vue";
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ConversationComposer from "../src/features/agent/components/ConversationComposer.vue";
import ConversationTranscript from "../src/features/agent/components/ConversationTranscript.vue";
import { useChatImageDrafts } from "../src/features/agent/useChatImageDrafts";
import type { ChatImageDraft, ChatMessage } from "../src/features/agent/types";
import {
  beginConversationTurn,
  clearConversationTurnPhase,
  confirmConversationTurn,
  failConversationTurn,
  getConversationRuntime,
} from "../src/features/agent/conversationRuntimeStore";

const bridge = vi.hoisted(() => ({
  discardImageDrafts: vi.fn(async () => undefined),
  normalizeCommandError: vi.fn((error: unknown) => ({ code: "test", message: String(error) })),
  prepareImages: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

const draft: ChatImageDraft = {
  draftId: "draft-1",
  id: "image-1",
  name: "face.png",
  path: "C:\\managed\\face.png",
  mime: "image/png",
  size: 128,
  available: true,
};

beforeEach(() => {
  bridge.discardImageDrafts.mockReset().mockResolvedValue(undefined);
  bridge.prepareImages.mockReset().mockResolvedValue({ accepted: [draft], rejected: [] });
});

describe("聊天图片", () => {
  it("按剩余数量准备路径并把部分拒绝反馈给用户", async () => {
    const drafts = ref<ChatImageDraft[]>([]);
    const error = ref<string | null>(null);
    bridge.prepareImages.mockResolvedValue({
      accepted: [draft],
      rejected: [{ index: 1, name: "bad.bmp", code: "image_unsupported", message: "格式不支持" }],
    });
    const images = useChatImageDrafts({
      drafts,
      canInteract: () => true,
      setError: (message) => {
        error.value = message;
      },
    });

    await images.addPaths(["C:\\face.png", "C:\\bad.bmp"]);

    expect(bridge.prepareImages).toHaveBeenCalledWith(
      [
        { kind: "path", path: "C:\\face.png" },
        { kind: "path", path: "C:\\bad.bmp" },
      ],
      8,
    );
    expect(drafts.value).toEqual([draft]);
    expect(error.value).toContain("bad.bmp：格式不支持");
  });

  it("普通文本粘贴保持默认行为，图片粘贴才拦截并准备字节", async () => {
    const drafts = ref<ChatImageDraft[]>([]);
    const images = useChatImageDrafts({
      drafts,
      canInteract: () => true,
      setError: () => undefined,
    });
    const preventText = vi.fn();
    await images.pasteImages({
      clipboardData: { items: [{ kind: "string", type: "text/plain" }] },
      preventDefault: preventText,
    } as unknown as ClipboardEvent);
    expect(preventText).not.toHaveBeenCalled();
    expect(bridge.prepareImages).not.toHaveBeenCalled();

    const preventImage = vi.fn();
    const file = {
      name: "pasted.png",
      type: "image/png",
      arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer,
    } as File;
    await images.pasteImages({
      clipboardData: {
        items: [{ kind: "file", type: "image/png", getAsFile: () => file }],
      },
      preventDefault: preventImage,
    } as unknown as ClipboardEvent);
    expect(preventImage).toHaveBeenCalledOnce();
    expect(bridge.prepareImages).toHaveBeenCalledWith(
      [expect.objectContaining({ kind: "bytes", name: "pasted.png" })],
      8,
    );
  });

  it("草稿缩略图支持查看和移除，纯图片也能触发发送", async () => {
    const view = render(ConversationComposer, {
      props: { modelValue: "", images: [draft], canSend: true },
    });
    await fireEvent.click(screen.getByRole("button", { name: "查看 face.png" }));
    await fireEvent.click(screen.getByRole("button", { name: "移除 face.png" }));
    await fireEvent.click(screen.getByRole("button", { name: "发送" }));

    expect(view.emitted().viewImage?.[0]).toEqual([draft]);
    expect(view.emitted().removeImage?.[0]).toEqual([draft.draftId]);
    expect(view.emitted().send).toHaveLength(1);
  });

  it("历史图片可查看，缺失图片显示不可用状态", async () => {
    const message: ChatMessage = {
      id: "message-1",
      role: "user",
      content: "",
      toolName: null,
      toolDisplayName: null,
      toolStatus: null,
      attachments: [
        draft,
        { ...draft, id: "missing", name: "missing.gif", available: false },
      ],
      createdAt: "2026-07-20T00:00:00Z",
    };
    const view = render(ConversationTranscript, { props: { messages: [message] } });
    await fireEvent.click(screen.getByRole("button", { name: "查看 face.png" }));

    expect(view.emitted().viewImage?.[0]).toEqual([draft]);
    expect(screen.getByText("图片不可用")).toBeTruthy();
    expect(screen.getByText("missing.gif")).toBeTruthy();
  });

  it("发送失败恢复文本和图片，成功后用持久消息替换乐观消息", () => {
    clearConversationTurnPhase("image-turn");
    const optimistic = beginConversationTurn("image-turn", "分析", [draft]);
    failConversationTurn("image-turn", optimistic, "分析", [draft], "发送失败");
    const failed = getConversationRuntime("image-turn");
    expect(failed.draft).toBe("分析");
    expect(failed.imageDrafts).toEqual([draft]);
    expect(failed.messages).toEqual([]);

    const retried = beginConversationTurn("image-turn", "分析", [draft]);
    const persisted: ChatMessage = {
      id: "persisted",
      role: "user",
      content: "分析",
      toolName: null,
      toolDisplayName: null,
      toolStatus: null,
      attachments: [{ ...draft }],
      createdAt: "2026-07-20T00:00:00Z",
    };
    confirmConversationTurn("image-turn", retried, persisted);
    expect(getConversationRuntime("image-turn").messages).toEqual([persisted]);
    clearConversationTurnPhase("image-turn");
  });
});
