import { beforeEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({
  getLlmConfig: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

describe("共享模型配置状态", () => {
  beforeEach(() => {
    vi.resetModules();
    bridge.getLlmConfig.mockReset();
  });

  it("模型就绪后自检优先展示 Editor 未连接", async () => {
    bridge.getLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: "example-model",
      hasApiKey: true,
    });
    const { editorStatus, modelStatus, store } = await setupStore();

    await store.initialize();

    expect(modelStatus).toMatchObject({ label: "example-model", tone: "ok", to: "/settings?tab=model-config" });
    expect(editorStatus).toMatchObject({ label: "Editor 未连接", tone: "warn", to: "/settings?tab=editor" });
  });

  it("读取失败时呈现可重试的模型异常状态", async () => {
    bridge.getLlmConfig.mockRejectedValue(new Error("unavailable"));
    const { modelStatus, store } = await setupStore();

    await expect(store.initialize()).rejects.toThrow("unavailable");

    expect(modelStatus).toMatchObject({
      label: "模型状态异常",
      tone: "error",
      to: "/settings?tab=model-config",
    });
  });
});

async function setupStore() {
  const { editorFooterStatus, modelFooterStatus } = await import("../src/features/shell/footerSelfCheck");
  const { useLlmConfigStore } = await import("../src/features/agent/llmConfigStore");
  return {
    store: useLlmConfigStore(),
    editorStatus: editorFooterStatus,
    modelStatus: modelFooterStatus,
  };
}
