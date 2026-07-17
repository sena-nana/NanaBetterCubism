import { beforeEach, describe, expect, it, vi } from "vitest";
import type { EditorSnapshot } from "../src/features/editor/types";

let stateListener: ((snapshot: EditorSnapshot) => void) | null = null;

const bridge = vi.hoisted(() => ({
  connectEditor: vi.fn(),
  disconnectEditor: vi.fn(),
  getEditorSnapshot: vi.fn(),
  listenEditorState: vi.fn(),
  normalizeCommandError: vi.fn((error: unknown) => ({
    code: "test_error",
    message: error instanceof Error ? error.message : String(error),
  })),
}));

vi.mock("../src/features/editor/bridge", () => bridge);

describe("Editor 侧栏状态", () => {
  beforeEach(() => {
    vi.resetModules();
    stateListener = null;
    bridge.getEditorSnapshot.mockReset();
    bridge.listenEditorState.mockReset();
    bridge.listenEditorState.mockImplementation(async (listener) => {
      stateListener = listener;
      return () => undefined;
    });
  });

  it("模型未就绪时 Editor 快照不抢占自检展示", async () => {
    bridge.getEditorSnapshot.mockResolvedValue(snapshot("ready", "Editor 已连接。"));
    const { appConfig } = await import("../src/app.config");
    const { setLiliaAppConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui");
    const { useEditorStore } = await import("../src/features/editor/editorStore");
    setLiliaAppConfig(appConfig);

    await useEditorStore().initialize();

    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck")).toMatchObject({
      label: "模型读取中",
      tone: "warn",
      to: "/settings?tab=model-config",
    });

    stateListener?.(snapshot("failed", "Editor 连接失败。"));
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck")).toMatchObject({
      label: "模型读取中",
      tone: "warn",
    });
  });

  it("模型就绪后自检跟随 Editor 状态，双端 OK 时显示就绪", async () => {
    bridge.getEditorSnapshot.mockResolvedValue(snapshot("ready", "Editor 已连接。"));
    const { appConfig } = await import("../src/app.config");
    const { setLiliaAppConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui");
    const { publishModelFooter } = await import("../src/features/shell/footerSelfCheck");
    const { useEditorStore } = await import("../src/features/editor/editorStore");
    setLiliaAppConfig(appConfig);

    publishModelFooter({
      label: "example-model",
      title: "已保存模型 example-model。点击进入设置。",
      tone: "ok",
    });
    await useEditorStore().initialize();

    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck")).toMatchObject({
      label: "就绪",
      tone: "ok",
    });

    stateListener?.(snapshot("failed", "Editor 连接失败。"));
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck")).toMatchObject({
      label: "Editor 连接异常",
      tone: "error",
      to: "/settings?tab=editor",
    });
  });

  it("初始化失败后重复进入设置仍保留异常状态", async () => {
    bridge.getEditorSnapshot.mockRejectedValue(new Error("unavailable"));
    const { appConfig } = await import("../src/app.config");
    const { setLiliaAppConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui");
    const { publishModelFooter } = await import("../src/features/shell/footerSelfCheck");
    const { useEditorStore } = await import("../src/features/editor/editorStore");
    setLiliaAppConfig(appConfig);
    publishModelFooter({
      label: "example-model",
      title: "已保存模型 example-model。点击进入设置。",
      tone: "ok",
    });
    const store = useEditorStore();

    await store.initialize();
    await store.initialize();

    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "selfcheck")).toMatchObject({
      label: "Editor 状态异常",
      tone: "error",
    });
  });
});

function snapshot(state: EditorSnapshot["state"], message: string): EditorSnapshot {
  return {
    state,
    port: 22033,
    apiVersion: null,
    modelLabel: null,
    groups: [],
    capabilities: {
      batchCreateParameters: false,
      findPartParameters: false,
      officialApi: false,
      officialEditApi: false,
    },
    message,
  };
}
