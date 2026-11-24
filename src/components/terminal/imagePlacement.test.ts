import { describe, expect, it } from "vitest";
import type { ImageRun } from "../../lib/types";
import { applyRowImages, drawParams, runRects } from "./imagePlacement";

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

    expect(rects).not.toBeNull();
    expect(rects?.sx).toBeCloseTo(20);
    expect(rects?.sy).toBeCloseTo(0);
    expect(rects?.sw).toBeCloseTo(70);
    expect(rects?.sh).toBeCloseTo(20);
    expect(rects?.dx).toBeCloseTo(30);
    expect(rects?.dy).toBeCloseTo(2);
    expect(rects?.dw).toBeCloseTo(66);
    expect(rects?.dh).toBeCloseTo(20);
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

    expect(rects?.dx).toBeCloseTo(33);
    expect(rects?.dy).toBeCloseTo(7);
    expect(rects?.dw).toBeCloseTo(63);
    expect(rects?.dh).toBeCloseTo(13);
  });

  it("returns null when horizontal padding consumes the whole span", () => {
    expect(runRects(run({ span: 1, padLeft: 6, padRight: 4 }), 100, 50, 10, 20, 0)).toBeNull();
    expect(runRects(run({ span: 1, padLeft: 8, padRight: 8 }), 100, 50, 10, 20, 0)).toBeNull();
  });

  it("returns null when vertical padding consumes the whole cell height", () => {
    expect(runRects(run({ padTop: 12, padBottom: 8 }), 100, 50, 10, 20, 0)).toBeNull();
    expect(runRects(run({ padTop: 15, padBottom: 15 }), 100, 50, 10, 20, 0)).toBeNull();
  });

  it("returns null for a non-positive source size", () => {
    expect(runRects(run({ u0: 0.5, u1: 0.5 }), 100, 50, 10, 20, 0)).toBeNull();
    expect(runRects(run({ v0: 0.8, v1: 0.2 }), 100, 50, 10, 20, 0)).toBeNull();
    expect(runRects(run(), 0, 50, 10, 20, 0)).toBeNull();
  });

  it("clamps u and v to 0..1 before mapping to source pixels", () => {
    const rects = runRects(run({ u0: -0.5, u1: 1.5, v0: -1, v1: 2 }), 100, 50, 10, 20, 0);

    expect(rects).not.toBeNull();
    expect(rects?.sx).toBeCloseTo(0);
    expect(rects?.sy).toBeCloseTo(0);
    expect(rects?.sw).toBeCloseTo(100);
    expect(rects?.sh).toBeCloseTo(50);
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

describe("drawParams", () => {
  const rect = (over: Partial<Parameters<typeof drawParams>[0]> = {}) => ({
    sx: 10.4,
    sy: 20.6,
    sw: 100.3,
    sh: 50.2,
    dx: 5,
    dy: 7,
    dw: 100.3,
    dh: 50.2,
    ...over,
  });

  it("turns smoothing off at 1:1 and leaves the source rect untouched", () => {
    const r = rect();
    expect(drawParams(r)).toEqual({ smooth: false, sx: 10.4, sy: 20.6, sw: 100.3, sh: 50.2 });
  });

  it("treats a scale within 1% on both axes as 1:1", () => {
    const r = rect({ dw: 100.3 * 1.009, dh: 50.2 * 0.991 });
    expect(drawParams(r).smooth).toBe(false);
  });

  it("turns smoothing on when the width is off by more than 1%", () => {
    expect(drawParams(rect({ dw: 100.3 * 1.02 })).smooth).toBe(true);
  });

  it("turns smoothing on when only the height is off by more than 1%", () => {
    expect(drawParams(rect({ dh: 50.2 * 0.98 })).smooth).toBe(true);
  });

  it("turns smoothing on when downscaling", () => {
    expect(drawParams(rect({ dw: 50, dh: 25 })).smooth).toBe(true);
  });

  it("snaps the source rect outward to whole pixels when smoothing", () => {
    const p = drawParams(rect({ dw: 50, dh: 25 }));
    expect(p).toEqual({ smooth: true, sx: 10, sy: 20, sw: 101, sh: 51 });
    // 10.4 + 100.3 and 20.6 + 50.2 round up to the next whole pixel edge
    expect(p.sx + p.sw).toBe(111);
    expect(p.sy + p.sh).toBe(71);
  });

  it("keeps a source rect that is already whole", () => {
    const p = drawParams({ sx: 4, sy: 8, sw: 40, sh: 16, dx: 0, dy: 0, dw: 20, dh: 8 });
    expect(p).toEqual({ smooth: true, sx: 4, sy: 8, sw: 40, sh: 16 });
  });
});
