import { describe, expect, it, vi } from "vitest";
import { addDroppedChatPaths } from "../src/features/agent/chatDropPaths";

describe("聊天文件拖放分流", () => {
  it("按扩展名把 PSD 与图片交给各自处理器", async () => {
    const addImages = vi.fn(async () => [] as string[]);
    const addPsds = vi.fn(async () => [] as string[]);
    const setError = vi.fn();

    await addDroppedChatPaths(
      ["C:\\face.png", "C:\\model.psd", "C:\\upper.PSD", "C:\\photo.webp"],
      { addImages, addPsds, setError },
    );

    expect(addImages).toHaveBeenCalledWith(["C:\\face.png", "C:\\photo.webp"]);
    expect(addPsds).toHaveBeenCalledWith(["C:\\model.psd", "C:\\upper.PSD"]);
    expect(setError).toHaveBeenCalledWith(null);
  });

  it("混合拖放保留两类成功结果并合并部分失败", async () => {
    const inserted: string[] = [];
    const addImages = vi.fn(async (paths: string[]) => {
      inserted.push(paths[0]);
      return ["图片失败"];
    });
    const addPsds = vi.fn(async (paths: string[]) => {
      inserted.push(paths[0]);
      return ["PSD 失败"];
    });
    const setError = vi.fn();

    await addDroppedChatPaths(["C:\\face.png", "C:\\model.psd"], {
      addImages,
      addPsds,
      setError,
    });

    expect(inserted).toEqual(expect.arrayContaining(["C:\\face.png", "C:\\model.psd"]));
    const error = setError.mock.calls[0][0] as string;
    expect(error).toContain("图片失败");
    expect(error).toContain("PSD 失败");
  });
});
