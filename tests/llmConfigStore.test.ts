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

  it("加载配置后只更新模型 footer", async () => {
    bridge.getLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: "example-model",
      hasApiKey: true,
    });
    const { store, statuses } = await setupStore();

    await store.initialize();

    expect(statuses.find((status) => status.key === "model")).toMatchObject({
      label: "example-model",
      tone: "ok",
    });
    expect(statuses.find((status) => status.key === "editor")).toMatchObject({
      label: "Editor 未连接",
      tone: "warn",
    });
  });

  it("读取失败时呈现可重试的模型异常状态", async () => {
    bridge.getLlmConfig.mockRejectedValue(new Error("unavailable"));
    const { store, statuses } = await setupStore();

    await expect(store.initialize()).rejects.toThrow("unavailable");

    expect(statuses.find((status) => status.key === "model")).toMatchObject({
      label: "模型状态异常",
      tone: "error",
      to: "/settings?tab=model-config",
    });
  });
});

async function setupStore() {
  const { appConfig } = await import("../src/app.config");
  const { setLiliaUiConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui/shell");
  const { useLlmConfigStore } = await import("../src/features/agent/llmConfigStore");
  setLiliaUiConfig(appConfig);
  return {
    store: useLlmConfigStore(),
    statuses: SIDEBAR_FOOTER_STATUSES,
  };
}
