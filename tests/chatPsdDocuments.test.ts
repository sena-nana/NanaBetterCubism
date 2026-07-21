import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useChatPsdDocuments } from "../src/features/agent/useChatPsdDocuments";
import type { ChatPsdDocument } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  normalizeCommandError: vi.fn((error: unknown) => ({
    code: "test",
    message: String(error),
  })),
  preparePsd: vi.fn(),
  discardPsd: vi.fn(),
  listPsds: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../src/features/editor/bridge", () => ({
  isTauriRuntime: () => true,
  domainError: (code: string, message: string) => ({ code, message }),
}));

const document: ChatPsdDocument = {
  id: "psd-1",
  name: "face.psd",
  path: "C:\\managed\\face.psd",
  width: 1024,
  height: 768,
  colorMode: "rgb",
  layerCount: 4,
  available: true,
};

beforeEach(() => {
  bridge.preparePsd.mockReset();
  bridge.discardPsd.mockReset();
  bridge.listPsds.mockReset();
});

describe("聊天 PSD 文档", () => {
  it("选取 PSD 后写入文档列表", async () => {
    const documents = ref<ChatPsdDocument[]>([]);
    const error = ref<string | null>(null);
    bridge.preparePsd.mockResolvedValue({ document, structure: { layers: [] } } as never);
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("C:\\face.psd" as never);

    const controller = useChatPsdDocuments({
      conversationId: () => "conv-1",
      documents,
      canInteract: () => true,
      setError: (message) => {
        error.value = message;
      },
    });

    await controller.pickPsd();

    expect(bridge.preparePsd).toHaveBeenCalledWith("conv-1", "C:\\face.psd");
    expect(documents.value).toEqual([document]);
    expect(error.value).toBeNull();
  });

  it("移除 PSD 后用后端返回的列表覆盖本地状态", async () => {
    const documents = ref<ChatPsdDocument[]>([document]);
    const error = ref<string | null>(null);
    bridge.discardPsd.mockResolvedValue([] as never);

    const controller = useChatPsdDocuments({
      conversationId: () => "conv-1",
      documents,
      canInteract: () => true,
      setError: (message) => {
        error.value = message;
      },
    });

    await controller.removePsd(document.id);

    expect(bridge.discardPsd).toHaveBeenCalledWith("conv-1", document.id);
    expect(documents.value).toEqual([]);
  });

  it("移除失败时回滚本地状态并上报错误", async () => {
    const documents = ref<ChatPsdDocument[]>([document]);
    const error = ref<string | null>(null);
    bridge.discardPsd.mockRejectedValue(new Error("boom") as never);

    const controller = useChatPsdDocuments({
      conversationId: () => "conv-1",
      documents,
      canInteract: () => true,
      setError: (message) => {
        error.value = message;
      },
    });

    await controller.removePsd(document.id);

    expect(documents.value).toEqual([document]);
    expect(error.value).toContain("boom");
  });
});
