import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { SIDEBAR_FOOTER_STATUSES, setLiliaUiConfig } from "@lilia/ui/shell";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { appConfig } from "../src/app.config";
import LlmSettingsSection from "../src/features/agent/settings/LlmSettingsSection.vue";

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

describe("模型配置", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    bridge.getLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: "example-model",
      hasApiKey: true,
    });
    setLiliaUiConfig(appConfig);
  });

  it("保存配置失败时不会继续测试旧配置", async () => {
    bridge.setLlmConfig.mockRejectedValue(new Error("无法保存配置"));

    render(LlmSettingsSection);
    await screen.findByDisplayValue("https://api.example.test/v1");
    await fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(bridge.setLlmConfig).toHaveBeenCalledTimes(1));
    expect(bridge.testLlmConnection).not.toHaveBeenCalled();
    expect(await screen.findByText("无法保存配置")).toBeTruthy();
  });

  it("保存成功后测试连接，并展示服务返回的可用模型", async () => {
    bridge.setLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: "example-model",
      hasApiKey: true,
    });
    bridge.testLlmConnection.mockResolvedValue({
      ok: true,
      message: "连接成功，对话测试通过。",
      models: ["example-model", "example-model-mini"],
    });

    render(LlmSettingsSection);
    await screen.findByDisplayValue("https://api.example.test/v1");
    await fireEvent.click(screen.getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(bridge.testLlmConnection).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("button", { name: "example-model" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "example-model-mini" })).toBeTruthy();
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "model")).toMatchObject({
      label: "example-model",
      tone: "ok",
    });
  });

  it("清除密钥后立即将共享模型状态恢复为未配置", async () => {
    bridge.setLlmConfig.mockResolvedValue({
      baseUrl: "https://api.example.test/v1",
      model: "example-model",
      hasApiKey: false,
    });

    render(LlmSettingsSection);
    await screen.findByDisplayValue("https://api.example.test/v1");
    await fireEvent.click(screen.getByRole("button", { name: "清除密钥" }));

    await waitFor(() => {
      expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "model")).toMatchObject({
        label: "模型未配置",
        tone: "warn",
      });
    });
  });
});
