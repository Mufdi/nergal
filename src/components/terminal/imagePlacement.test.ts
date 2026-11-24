import { describe, expect, it } from "vitest";
import type { ImageRun } from "../../lib/types";
import { applyRowImages, runRects } from "./imagePlacement";

function run(over: Partial<ImageRun> = {}): ImageRun {
  return {
    hash: "a".repeat(64),
    col: 0,
    span: 1,
    u0: 0,
    v0: 0,
    u1: 1,
    v1: 1,
    padLeft: 0,
    padTop: 0,
    padRight: 0,
    padBottom: 0,
    z: 0,
    ...over,
  };
}

describe("runRects", () => {
  it("maps a partial run on a 100x50 image into device pixels", () => {
    const rects = runRects(
      run({ col: 3, span: 7, u0: 0.2, u1: 0.9, v0: 0, v1: 0.4, padRight: 4 }),
      100,
      50,
      10,
      20,
      2,
    );

    expect(rects.sx).toBeCloseTo(20);
    expect(rects.sy).toBeCloseTo(0);
    expect(rects.sw).toBeCloseTo(70);
    expect(rects.sh).toBeCloseTo(20);
    expect(rects.dx).toBeCloseTo(30);
    expect(rects.dy).toBeCloseTo(2);
    expect(rects.dw).toBeCloseTo(66);
    expect(rects.dh).toBeCloseTo(20);
  });

  it("shrinks the destination by the left, top and bottom padding too", () => {
    const rects = runRects(
      run({ col: 3, span: 7, padLeft: 3, padTop: 5, padRight: 4, padBottom: 2 }),
      100,
      50,
      10,
      20,
      2,
    );

    expect(rects.dx).toBeCloseTo(33);
    expect(rects.dy).toBeCloseTo(7);
    expect(rects.dw).toBeCloseTo(63);
    expect(rects.dh).toBeCloseTo(13);
  });
});

describe("applyRowImages", () => {
  const a = run({ hash: "a".repeat(64) });
  const b = run({ hash: "b".repeat(64) });

  it("clears the runs of a received row that carries no images", () => {
    const next = applyRowImages([[a], [b], []], {
      totalRows: 3,
      cols: 80,
      rows: [{ index: 0 }],
    });

    expect(next[0]).toEqual([]);
    expect(next[1]).toEqual([b]);
  });

  it("replaces the runs of a received row that carries images", () => {
    const next = applyRowImages([[a], []], {
      totalRows: 2,
      cols: 80,
      rows: [{ index: 0, images: [b] }],
    });

    expect(next[0]).toEqual([b]);
  });

  it("keeps the runs of rows the update does not mention", () => {
    const next = applyRowImages([[a], [b], []], {
      totalRows: 3,
      cols: 80,
      rows: [{ index: 2 }],
    });

    expect(next[0]).toEqual([a]);
    expect(next[1]).toEqual([b]);
  });

  it("reshapes to the new row count", () => {
    const shrunk = applyRowImages([[a], [b], []], { totalRows: 2, cols: 80, rows: [] });
    expect(shrunk).toHaveLength(2);

    const grown = applyRowImages([[a]], { totalRows: 4, cols: 80, rows: [] });
    expect(grown).toHaveLength(4);
    expect(grown.slice(1)).toEqual([[], [], []]);
  });

  it("drops runs that start at or past cols", () => {
    const inside = run({ col: 9, span: 1 });
    const atEdge = run({ col: 10, span: 2 });
    const past = run({ col: 12, span: 1 });

    const next = applyRowImages([[]], {
      totalRows: 1,
      cols: 10,
      rows: [{ index: 0, images: [inside, atEdge, past] }],
    });

    expect(next[0]).toEqual([inside]);
  });
});
