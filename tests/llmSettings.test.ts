import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LlmSettingsSection from "../src/features/agent/settings/LlmSettingsSection.vue";
import { useLlmConfigStore } from "../src/features/agent/llmConfigStore";
import { modelFooterStatus } from "../src/features/shell/footerSelfCheck";

const bridge = vi.hoisted(() => ({
  getLlmConfig: vi.fn(),
  setLlmConfig: vi.fn(),
  testLlmConnection: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => ({
  ...bridge,
  normalizeCommandError: (error: unknown) => ({
    code: "test_error",
    message: error instanceof Error ? error.message : String(error),
  }),
}));

const completeConfig = {
  baseUrl: "https://api.example.test/v1",
  model: "example-model",
  hasApiKey: true,
};

const successfulCheck = {
  ok: true,
  message: "连接成功，对话测试通过。",
  models: ["example-model", "example-model-mini"],
};

describe("模型配置", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockResolvedValue(successfulCheck);
    useLlmConfigStore().applyConfig(completeConfig);
  });

  afterEach(cleanup);

  it("保存配置失败时不会继续测试旧配置", async () => {
    bridge.setLlmConfig.mockRejectedValue(new Error("无法保存配置"));
    await renderSettings();
    bridge.testLlmConnection.mockClear();

    await fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(bridge.setLlmConfig).toHaveBeenCalledTimes(1));
    expect(bridge.testLlmConnection).not.toHaveBeenCalled();
    expect(await screen.findByText("无法保存配置")).toBeTruthy();
  });

  it("手动测试成功后更新共享状态并展示可用模型", async () => {
    bridge.setLlmConfig.mockResolvedValue(completeConfig);
    const { modelStatus, store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(bridge.testLlmConnection).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("button", { name: "example-model" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "example-model-mini" })).toBeTruthy();
    expect(store.state.connectionStatus).toBe("ready");
    expect(modelStatus.tone).toBe("ok");
  });

  it("保存完整配置后立即将共享状态标记为过期", async () => {
    bridge.setLlmConfig.mockResolvedValue({ ...completeConfig, model: "next-model" });
    const { modelStatus, store } = await renderSettings();
    bridge.testLlmConnection.mockClear();

    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(store.state.connectionStatus).toBe("stale"));
    expect(modelStatus.tone).toBe("warn");
    expect(bridge.testLlmConnection).not.toHaveBeenCalled();
  });

  it("清除密钥后立即将共享模型状态恢复为未配置", async () => {
    bridge.setLlmConfig.mockResolvedValue({ ...completeConfig, hasApiKey: false });
    const { modelStatus, store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "清除密钥" }));

    await waitFor(() => expect(store.state.connectionStatus).toBe("unconfigured"));
    expect(modelStatus.tone).toBe("warn");
  });

  it("自动选择首个模型后重新验证最终配置", async () => {
    bridge.getLlmConfig.mockResolvedValue({ ...completeConfig, model: null });
    bridge.setLlmConfig
      .mockResolvedValueOnce({ ...completeConfig, model: null })
      .mockResolvedValueOnce(completeConfig);
    const { modelStatus, store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(bridge.testLlmConnection).toHaveBeenCalledTimes(2));
    expect(bridge.setLlmConfig).toHaveBeenCalledTimes(2);
    expect(store.state.connectionStatus).toBe("ready");
    expect(modelStatus).toMatchObject({ label: "example-model", tone: "ok" });
  });
});

async function renderSettings() {
  render(LlmSettingsSection);
  await screen.findByDisplayValue("https://api.example.test/v1");
  return { modelStatus: modelFooterStatus, store: useLlmConfigStore() };
}
