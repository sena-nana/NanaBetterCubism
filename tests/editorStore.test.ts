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

  it("Editor 快照更新不会覆盖模型状态", async () => {
    bridge.getEditorSnapshot.mockResolvedValue(snapshot("ready", "Editor 已连接。"));
    const { appConfig } = await import("../src/app.config");
    const { setLiliaAppConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui");
    const { useEditorStore } = await import("../src/features/editor/editorStore");
    setLiliaAppConfig(appConfig);

    await useEditorStore().initialize();

    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "model")).toMatchObject({
      label: "模型读取中",
    });
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "editor")).toMatchObject({
      label: "Editor 已就绪",
      tone: "ok",
    });

    stateListener?.(snapshot("failed", "Editor 连接失败。"));
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "editor")).toMatchObject({
      label: "Editor 连接异常",
      tone: "error",
    });
    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "model")).toMatchObject({
      label: "模型读取中",
    });
  });

  it("初始化失败后重复进入设置仍保留异常状态", async () => {
    bridge.getEditorSnapshot.mockRejectedValue(new Error("unavailable"));
    const { appConfig } = await import("../src/app.config");
    const { setLiliaAppConfig, SIDEBAR_FOOTER_STATUSES } = await import("@lilia/ui");
    const { useEditorStore } = await import("../src/features/editor/editorStore");
    setLiliaAppConfig(appConfig);
    const store = useEditorStore();

    await store.initialize();
    await store.initialize();

    expect(SIDEBAR_FOOTER_STATUSES.find((status) => status.key === "editor")).toMatchObject({
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
