import { describe, expect, it } from "vitest";
import { createLocalPreview, makeRow } from "../src/features/parameters/utils/idTemplate";
import { parsePastedRows } from "../src/features/parameters/utils/pasteRows";

const config = {
  template: "{prefix}{key}{side}{index}{suffix}",
  prefix: "Param",
  suffix: "",
  startIndex: 1,
  indexWidth: 2,
};

describe("参数 ID 模板与粘贴导入", () => {
  it("展开编号并补零", () => {
    const rows = [
      makeRow({ name: "左眼", key: "Eye", side: "L" }),
      makeRow({ name: "右眼", key: "Eye", side: "R" }),
    ];

    const preview = createLocalPreview(config, rows);

    expect([...preview.ids.values()]).toEqual(["ParamEyeL01", "ParamEyeR02"]);
    expect(preview.errors).toEqual([]);
  });

  it("解析有表头的 TSV 和带引号的 CSV", () => {
    const tsv = parsePastedRows("名称\tID 段\t方位\n左眼\tEye\tL\n右眼\tEye\tR");
    const csv = parsePastedRows('"前,发",Hair,L');

    expect(tsv.map(({ name, key, side }) => ({ name, key, side }))).toEqual([
      { name: "左眼", key: "Eye", side: "L" },
      { name: "右眼", key: "Eye", side: "R" },
    ]);
    expect(csv[0]).toMatchObject({ name: "前,发", key: "Hair", side: "L" });
  });

  it("阻止非法模板、范围和超过 200 行的批次", () => {
    const rows = Array.from({ length: 201 }, (_, index) =>
      makeRow({ name: `参数 ${index}`, key: "Angle" }),
    );
    rows[0].overrides = { min: 1, default: 0, max: -1 };

    const preview = createLocalPreview({ ...config, template: "{unknown}" }, rows);
    const codes = new Set(preview.errors.map((error) => error.code));

    expect(codes.has("invalid_template")).toBe(true);
    expect(codes.has("invalid_range")).toBe(true);
    expect(codes.has("batch_too_large")).toBe(true);
  });

  it("阻止批内重复和不符合 Cubism 规则的 ID", () => {
    const rows = [
      makeRow({ name: "A", key: "" }),
      makeRow({ name: "B", key: "" }),
    ];
    const preview = createLocalPreview(
      { ...config, template: "{key}", startIndex: 0, indexWidth: 1 },
      rows,
    );

    expect(preview.errors.some((error) => error.code === "duplicate_id")).toBe(true);
    expect(preview.errors.some((error) => error.code === "invalid_id")).toBe(true);
  });
});
