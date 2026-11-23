import { describe, expect, it } from "vitest";
import { readingLayout } from "./readingCols";
import { nextResize, recordRequest, type CellMetrics } from "./pixelGeometry";
import type { FontMetrics } from "./fontAtlas";

const m: FontMetrics = { cellWidth: 8, cellHeight: 16, baseline: 12, cssWidth: 8, cssHeight: 16 };

describe("readingLayout", () => {
  it.each([
    ["wide pane is capped and centered", 1440, 800, "center", true, { cols: 100, rows: 50, centered: true }],
    ["narrow pane keeps its width", 640, 800, "center", true, { cols: 80, rows: 50, centered: false }],
    ["flag off uses the full pane", 1440, 800, "center", false, { cols: 180, rows: 50, centered: false }],
    ["quake ignores the flag", 1440, 800, "quake", true, { cols: 180, rows: 50, centered: false }],
    ["tiny width falls back to 80 columns before the cap", 40, 800, "center", true, { cols: 80, rows: 50, centered: false }],
    ["bogus height falls back to 24 rows and the width is still capped", 1440, 20, "center", true, { cols: 100, rows: 24, centered: true }],
  ] as const)("%s", (_name, w, h, region, enabled, expected) => {
    expect(readingLayout(w, h, m, region, enabled)).toEqual(expected);
  });
});

describe("resize dedup across the reading cap", () => {
  it("a request at 100 columns is sent after one at 180", () => {
    const cell: CellMetrics = { cellWidth: 8, cellHeight: 16 };
    const first = nextResize(null, "s1", 180, 50, cell);
    if (!first) throw new Error("expected a payload");
    const next = nextResize(recordRequest(first), "s1", 100, 50, cell);
    expect(next).not.toBeNull();
    expect(next?.cols).toBe(100);
  });
});
