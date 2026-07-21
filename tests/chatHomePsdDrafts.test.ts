import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useHomePsdDrafts } from "../src/features/agent/useHomePsdDrafts";
import type { ChatPsdDocument, ChatPsdDraft } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  normalizeCommandError: vi.fn((error: unknown) => ({
    code: "test",
    message: String(error),
  })),
  preparePsd: vi.fn(),
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
});

describe("首页 PSD 草稿", () => {
  it("选取 PSD 后写入草稿列表且不调用后端", async () => {
    const drafts = ref<ChatPsdDraft[]>([]);
    const error = ref<string | null>(null);
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("C:\\face.psd" as never);

    const controller = useHomePsdDrafts({
      drafts,
      canInteract: () => true,
      setError: (message) => {
        error.value = message;
      },
    });

    await controller.pickPsd();

    expect(bridge.preparePsd).not.toHaveBeenCalled();
    expect(drafts.value).toHaveLength(1);
    expect(drafts.value[0].name).toBe("face.psd");
    expect(drafts.value[0].path).toBe("C:\\face.psd");
    expect(drafts.value[0].id).toBeTruthy();
    expect(error.value).toBeNull();
  });

  it("取消选取不写入草稿", async () => {
    const drafts = ref<ChatPsdDraft[]>([]);
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue(null as never);

    const controller = useHomePsdDrafts({
      drafts,
      canInteract: () => true,
      setError: () => {},
    });

    await controller.pickPsd();

    expect(drafts.value).toHaveLength(0);
  });

  it("removePsdDraft 移除对应草稿", () => {
    const drafts = ref<ChatPsdDraft[]>([
      { id: "a", name: "a.psd", path: "C:\\a.psd" },
      { id: "b", name: "b.psd", path: "C:\\b.psd" },
    ]);

    const controller = useHomePsdDrafts({
      drafts,
      canInteract: () => true,
      setError: () => {},
    });

    controller.removePsdDraft("a");

    expect(drafts.value).toEqual([{ id: "b", name: "b.psd", path: "C:\\b.psd" }]);
  });

  it("prepareAll 按顺序对每条草稿调用 preparePsd", async () => {
    const drafts = ref<ChatPsdDraft[]>([
      { id: "a", name: "a.psd", path: "C:\\a.psd" },
      { id: "b", name: "b.psd", path: "C:\\b.psd" },
    ]);
    bridge.preparePsd
      .mockResolvedValueOnce({ document: { ...document, id: "a", name: "a.psd", path: "C:\\a.psd" }, structure: { layers: [] } } as never)
      .mockResolvedValueOnce({ document: { ...document, id: "b", name: "b.psd", path: "C:\\b.psd" }, structure: { layers: [] } } as never);

    const controller = useHomePsdDrafts({
      drafts,
      canInteract: () => true,
      setError: () => {},
    });

    const prepared = await controller.prepareAll("conv-1");

    expect(bridge.preparePsd).toHaveBeenNthCalledWith(1, "conv-1", "C:\\a.psd");
    expect(bridge.preparePsd).toHaveBeenNthCalledWith(2, "conv-1", "C:\\b.psd");
    expect(prepared.map((item) => item.id)).toEqual(["a", "b"]);
  });

  it("prepareAll 在某条失败时抛错并停止后续", async () => {
    const drafts = ref<ChatPsdDraft[]>([
      { id: "a", name: "a.psd", path: "C:\\a.psd" },
      { id: "b", name: "b.psd", path: "C:\\b.psd" },
    ]);
    bridge.preparePsd.mockRejectedValueOnce(new Error("bad") as never);

    const controller = useHomePsdDrafts({
      drafts,
      canInteract: () => true,
      setError: () => {},
    });

    await expect(controller.prepareAll("conv-1")).rejects.toThrow("bad");
    expect(bridge.preparePsd).toHaveBeenCalledTimes(1);
  });
});
