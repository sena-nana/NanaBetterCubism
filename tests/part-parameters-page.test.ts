import { fireEvent, render, screen } from "@testing-library/vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  findSelectedPartParameters: vi.fn(),
  session: {
    state: {
      snapshot: {
        state: "ready",
        port: 22033,
        apiVersion: "1.1.0",
        modelLabel: "当前建模模型",
        groups: [],
        capabilities: { batchCreateParameters: true, findPartParameters: true },
        message: "已连接，可以校验并创建参数。",
      },
      error: null,
      initialized: true,
      busy: false,
    },
    canCreateParameters: { value: true },
    canFindPartParameters: { value: true },
    initialize: vi.fn(),
    connect: vi.fn(),
    disconnect: vi.fn(),
    clearError: vi.fn(),
  },
}));

vi.mock("../src/features/parameters/editorStore", () => ({
  useEditorStore: () => mocks.session,
}));

vi.mock("../src/features/part-parameters/bridge", () => ({
  findSelectedPartParameters: mocks.findSelectedPartParameters,
}));

import PartParametersPage from "../src/features/part-parameters/PartParametersPage.vue";

const result = {
  modelLabel: "当前建模模型",
  selectedParts: [{ id: "PartFace", name: "Face" }],
  ignoredSelectionCount: 1,
  scannedObjectCount: 2,
  parameters: [
    {
      id: "ParamEyeOpen",
      name: "Eye Open",
      group: { id: "ParamGroupFace", name: "Face" },
      keyValues: [0, 1],
      objects: [
        {
          id: "ArtEye",
          name: "Eye ArtMesh",
          objectType: "ArtMesh",
          keyValues: [0, 1],
          sourcePartIds: ["PartFace"],
        },
      ],
    },
  ],
};

describe("部件关联参数页面", () => {
  beforeEach(() => {
    mocks.findSelectedPartParameters.mockReset();
  });

  it("展示聚合参数并按需展开对象明细", async () => {
    mocks.findSelectedPartParameters.mockResolvedValue(result);
    render(PartParametersPage);

    await fireEvent.click(screen.getByRole("button", { name: "查找关联参数" }));

    expect(await screen.findByText("Eye Open")).toBeInTheDocument();
    expect(screen.getByText("已忽略 1 个非 Part 选择。")).toBeInTheDocument();
    expect(screen.queryByText("Eye ArtMesh")).not.toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "查看对象" }));
    expect(await screen.findByText("Eye ArtMesh")).toBeInTheDocument();
    expect(screen.getByText("来源 Face")).toBeInTheDocument();
  });

  it("将没有参数作为成功的空结果展示", async () => {
    mocks.findSelectedPartParameters.mockResolvedValue({ ...result, parameters: [] });
    render(PartParametersPage);

    await fireEvent.click(screen.getByRole("button", { name: "查找关联参数" }));

    expect(await screen.findByText("没有关联参数")).toBeInTheDocument();
  });

  it("展示结构化查询错误且不保留旧结果", async () => {
    mocks.findSelectedPartParameters.mockRejectedValue({
      code: "no_selected_part",
      message: "请在 Cubism Editor 中选择至少一个 Part。",
    });
    render(PartParametersPage);

    await fireEvent.click(screen.getByRole("button", { name: "查找关联参数" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("选择至少一个 Part");
    expect(screen.queryByText("Eye Open")).not.toBeInTheDocument();
  });
});
