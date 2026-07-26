import { beforeEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({
  getLlmConfig: vi.fn(),
  listLlmModels: vi.fn(),
  testLlmConnection: vi.fn(),
  testLlmModel: vi.fn(),
  listenImageCapability: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

const completeConfig = {
  baseUrl: "https://api.example.test/v1",
  model: "example-model",
  hasApiKey: true,
};

const successfulCheck = {
  ok: true,
  message: "connected",
};

describe("共享模型连接状态", () => {
  beforeEach(() => {
    vi.resetModules();
    bridge.getLlmConfig.mockReset();
    bridge.listLlmModels.mockReset();
    bridge.testLlmConnection.mockReset();
    bridge.testLlmModel.mockReset();
  });

  it("仅在完整配置通过真实连接检查后标记就绪", async () => {
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockResolvedValue(successfulCheck);
    const { editorStatus, modelStatus, store } = await setupStore();

    await store.initialize();

    expect(bridge.testLlmConnection).toHaveBeenCalledTimes(1);
    expect(store.state.connectionStatus).toBe("ready");
    expect(modelStatus).toMatchObject({ label: "example-model", tone: "ok" });
    expect(editorStatus).toMatchObject({ tone: "warn", to: "/settings?tab=editor" });
  });

  it("连接测试返回失败后重复初始化仍保留失败状态", async () => {
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockResolvedValue({
      ok: false,
      message: "unavailable",
    });
    const { modelStatus, store } = await setupStore();

    await store.initialize();
    await store.initialize();

    expect(bridge.testLlmConnection).toHaveBeenCalledTimes(1);
    expect(store.state.connectionStatus).toBe("failed");
    expect(modelStatus.tone).toBe("error");
  });

  it("连接命令异常时记录失败但不让配置初始化失败", async () => {
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockRejectedValue(new Error("unavailable"));
    const { modelStatus, store } = await setupStore();

    await expect(store.initialize()).resolves.toEqual(completeConfig);

    expect(store.state.connectionStatus).toBe("failed");
    expect(modelStatus.tone).toBe("error");
  });

  it("API 已配置但未选模型时保持待选择且不发起自动检查", async () => {
    bridge.getLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: null,
      hasApiKey: true,
    });
    const { modelStatus, store } = await setupStore();

    await store.initialize();

    expect(bridge.testLlmConnection).not.toHaveBeenCalled();
    expect(store.state.connectionStatus).toBe("model_required");
    expect(modelStatus.tone).toBe("warn");
  });

  it("API 检查成功后发布模型列表但不自动选择或标记就绪", async () => {
    bridge.getLlmConfig.mockResolvedValue({ ...completeConfig, model: null });
    bridge.listLlmModels.mockResolvedValue({
      models: ["example-model", "example-model-mini"],
    });
    const { modelStatus, store } = await setupStore();
    await store.initialize();

    const result = await store.discoverModels();

    expect(result?.models).toEqual(["example-model", "example-model-mini"]);
    expect(store.state.config.model).toBeNull();
    expect(store.state.connectionStatus).toBe("model_required");
    expect(modelStatus.tone).toBe("warn");
  });

  it("候选模型测试成功后才应用后端返回的已保存配置", async () => {
    bridge.getLlmConfig.mockResolvedValue({ ...completeConfig, model: null });
    bridge.testLlmModel.mockResolvedValue({
      ...successfulCheck,
      imageSupported: true,
      config: completeConfig,
    });
    const { modelStatus, store } = await setupStore();
    await store.initialize();

    await store.testModel("example-model");

    expect(bridge.testLlmModel).toHaveBeenCalledWith("example-model");
    expect(store.state.config.model).toBe("example-model");
    expect(store.state.connectionStatus).toBe("ready");
    expect(store.state.imageInputSupported).toBe(true);
    expect(modelStatus).toMatchObject({ label: "example-model", tone: "ok" });
  });

  it("候选模型失败时不应用配置且聊天保持不可用", async () => {
    bridge.getLlmConfig.mockResolvedValue({ ...completeConfig, model: null });
    bridge.testLlmModel.mockResolvedValue({
      ok: false,
      message: "unavailable",
    });
    const { modelStatus, store } = await setupStore();
    await store.initialize();

    await store.testModel("example-model");

    expect(store.state.config.model).toBeNull();
    expect(store.state.connectionStatus).toBe("failed");
    expect(modelStatus.tone).toBe("error");
  });

  it("配置写入会使成功结果过期，清除密钥后恢复未配置", async () => {
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockResolvedValue(successfulCheck);
    const { modelStatus, store } = await setupStore();
    await store.initialize();

    store.applyConfig({ ...completeConfig, model: "next-model" });

    expect(store.state.connectionStatus).toBe("stale");
    expect(modelStatus.tone).toBe("warn");

    store.applyConfig({ ...completeConfig, hasApiKey: false });

    expect(store.state.connectionStatus).toBe("unconfigured");
    expect(modelStatus.tone).toBe("warn");
  });

  it("忽略配置变更前尚未返回的检查结果", async () => {
    let resolveCheck!: (result: typeof successfulCheck) => void;
    bridge.getLlmConfig.mockResolvedValue(completeConfig);
    bridge.testLlmConnection.mockReturnValue(new Promise((resolve) => {
      resolveCheck = resolve;
    }));
    const { modelStatus, store } = await setupStore();

    const initializing = store.initialize();
    await vi.waitFor(() => expect(store.state.connectionStatus).toBe("checking"));
    store.applyConfig({ ...completeConfig, model: "next-model" });
    resolveCheck(successfulCheck);
    await initializing;

    expect(store.state.connectionStatus).toBe("stale");
    expect(modelStatus.tone).toBe("warn");
  });

  it("忽略配置变更前尚未返回的模型列表", async () => {
    let resolveModels!: (result: { models: string[] }) => void;
    bridge.getLlmConfig.mockResolvedValue({ ...completeConfig, model: null });
    bridge.listLlmModels.mockReturnValue(new Promise((resolve) => {
      resolveModels = resolve;
    }));
    const { store } = await setupStore();
    await store.initialize();

    const discovering = store.discoverModels();
    await vi.waitFor(() => expect(store.state.connectionStatus).toBe("checking"));
    store.applyConfig({ ...completeConfig, baseUrl: "https://next.example.test/v1" });
    resolveModels({ models: ["stale-model"] });
    const result = await discovering;

    expect(result).toBeNull();
    expect(store.state.connectionStatus).toBe("stale");
  });

  it("读取配置失败时呈现异常状态并允许调用方重试", async () => {
    bridge.getLlmConfig.mockRejectedValue(new Error("unavailable"));
    const { modelStatus, store } = await setupStore();

    await expect(store.initialize()).rejects.toThrow("unavailable");

    expect(store.state.connectionStatus).toBe("failed");
    expect(modelStatus).toMatchObject({
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
