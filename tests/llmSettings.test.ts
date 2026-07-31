import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LlmSettingsSection from "../src/features/agent/settings/LlmSettingsSection.vue";
import { useLlmConfigStore } from "../src/features/agent/llmConfigStore";
import { modelFooterStatus } from "../src/features/shell/footerSelfCheck";

const bridge = vi.hoisted(() => ({
  getLlmConfig: vi.fn(),
  setLlmConfig: vi.fn(),
  listLlmModels: vi.fn(),
  testLlmConnection: vi.fn(),
  testLlmModel: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => ({
  ...bridge,
  listenImageCapability: vi.fn().mockResolvedValue(() => {}),
  normalizeCommandError: (error: unknown) => ({
    code: "test_error",
    message: error instanceof Error ? error.message : String(error),
  }),
}));

const endpointConfig = {
  apiMode: "chat_completions" as const,
  baseUrl: "https://api.example.test/v1",
  model: null,
  hasApiKey: true,
};

const completeConfig = {
  ...endpointConfig,
  model: "example-model",
};

const models = ["example-model", "example-model-mini"];

describe("模型配置", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.listLlmModels.mockResolvedValue({ models });
    bridge.testLlmConnection.mockResolvedValue({
      ok: true,
      message: "连接成功，对话测试通过。",
    });
    bridge.testLlmModel.mockImplementation(async (model: string) => ({
      ok: true,
      message: "连接成功，对话测试通过。",
      config: { ...completeConfig, model },
    }));
    useLlmConfigStore().applyConfig(completeConfig);
  });

  afterEach(cleanup);

  it("不再提供手动模型输入", async () => {
    await renderSettings();

    expect(document.querySelector('[data-agent-id="settings.llm.model"]')).toBeNull();
    expect(screen.queryByRole("combobox", { name: "模型" })).toBeNull();
  });

  it("保存 API 配置失败时不会读取旧端点的模型列表", async () => {
    bridge.setLlmConfig.mockRejectedValue(new Error("无法保存配置"));
    await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));

    await waitFor(() => expect(bridge.setLlmConfig).toHaveBeenCalledTimes(1));
    expect(bridge.listLlmModels).not.toHaveBeenCalled();
    expect(bridge.testLlmModel).not.toHaveBeenCalled();
    expect(await screen.findByText("无法保存配置")).toBeTruthy();
  });

  it("API 测试后显示模型列表但不自动选择或测试模型", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    useLlmConfigStore().applyConfig(endpointConfig);
    const { store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));

    const select = await screen.findByRole("combobox", { name: "模型" });
    expect((select as HTMLSelectElement).value).toBe("");
    expect(screen.getByRole("option", { name: "example-model" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "example-model-mini" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "测试模型连接" })).toBeDisabled();
    expect(bridge.testLlmModel).not.toHaveBeenCalled();
    expect(store.state.connectionStatus).toBe("model_required");
  });

  it("已保存模型仍在列表中时只预选，不自动发起模型测试", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));

    const select = await screen.findByRole("combobox", { name: "模型" });
    expect((select as HTMLSelectElement).value).toBe("example-model");
    expect(bridge.testLlmModel).not.toHaveBeenCalled();
  });

  it("用户选择并显式测试成功后才保存模型并标记就绪", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    const { modelStatus, store } = await renderSettings();
    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));
    const select = await screen.findByRole("combobox", { name: "模型" });
    await fireEvent.update(select, "example-model-mini");

    await fireEvent.click(screen.getByRole("button", { name: "测试模型连接" }));

    await waitFor(() =>
      expect(bridge.testLlmModel).toHaveBeenCalledWith("example-model-mini"),
    );
    expect(store.state.config.model).toBe("example-model-mini");
    expect(store.state.connectionStatus).toBe("ready");
    expect(modelStatus).toMatchObject({ label: "example-model-mini", tone: "ok" });
  });

  it("模型测试失败时保持不可用且不应用候选模型", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    bridge.testLlmModel.mockResolvedValue({
      ok: false,
      message: "模型不可用",
    });
    const { store } = await renderSettings();
    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));
    const select = await screen.findByRole("combobox", { name: "模型" });
    await fireEvent.update(select, "example-model");

    await fireEvent.click(screen.getByRole("button", { name: "测试模型连接" }));

    expect(await screen.findByText("模型不可用")).toBeTruthy();
    expect(store.state.config.model).toBeNull();
    expect(store.state.connectionStatus).toBe("failed");
  });

  it("API 返回空列表时显示真实结果且不提供手输回退", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    bridge.listLlmModels.mockResolvedValue({ models: [] });
    await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));

    expect(await screen.findByText("API 已连接，但没有返回可用模型。")).toBeTruthy();
    expect(screen.queryByRole("combobox", { name: "模型" })).toBeNull();
    expect(screen.queryByRole("button", { name: "测试模型连接" })).toBeNull();
  });

  it("地址编辑后立即废弃已获取的模型列表", async () => {
    bridge.setLlmConfig.mockResolvedValue(endpointConfig);
    await renderSettings();
    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));
    await screen.findByRole("combobox", { name: "模型" });

    await fireEvent.update(
      screen.getByDisplayValue("https://api.example.test/v1"),
      "https://next.example.test/v1",
    );

    expect(screen.queryByRole("combobox", { name: "模型" })).toBeNull();
  });

  it("切换 API 类型后清除模型并保存新类型", async () => {
    bridge.setLlmConfig
      .mockResolvedValueOnce(endpointConfig)
      .mockResolvedValueOnce({
        ...endpointConfig,
        apiMode: "responses",
      });
    const { store } = await renderSettings();
    await fireEvent.click(screen.getByRole("button", { name: "测试 API 连接" }));
    await screen.findByRole("combobox", { name: "模型" });

    await fireEvent.update(
      screen.getByRole("combobox", { name: "API 类型" }),
      "responses",
    );

    expect(screen.queryByRole("combobox", { name: "模型" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(bridge.setLlmConfig).toHaveBeenCalledTimes(2));
    expect(bridge.setLlmConfig.mock.calls[1]?.[0]).toMatchObject({
      apiMode: "responses",
      model: null,
    });
    expect(store.state.config).toMatchObject({
      apiMode: "responses",
      model: null,
    });
    expect(store.state.connectionStatus).toBe("model_required");
  });

  it("保存完整配置后立即将共享状态标记为过期", async () => {
    bridge.setLlmConfig.mockResolvedValue({ ...completeConfig, model: "example-model" });
    const { modelStatus, store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(store.state.connectionStatus).toBe("stale"));
    expect(modelStatus.tone).toBe("warn");
    expect(bridge.listLlmModels).not.toHaveBeenCalled();
    expect(bridge.testLlmModel).not.toHaveBeenCalled();
  });

  it("清除密钥后立即将共享模型状态恢复为未配置", async () => {
    bridge.setLlmConfig.mockResolvedValue({
      ...endpointConfig,
      model: null,
      hasApiKey: false,
    });
    const { modelStatus, store } = await renderSettings();

    await fireEvent.click(screen.getByRole("button", { name: "清除密钥" }));

    await waitFor(() => expect(store.state.connectionStatus).toBe("unconfigured"));
    expect(modelStatus.tone).toBe("warn");
  });
});

async function renderSettings() {
  render(LlmSettingsSection);
  await screen.findByDisplayValue("https://api.example.test/v1");
  return { modelStatus: modelFooterStatus, store: useLlmConfigStore() };
}
