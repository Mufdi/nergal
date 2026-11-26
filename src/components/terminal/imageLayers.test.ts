import { describe, expect, it } from "vitest";
import type { ImageRun } from "../../lib/types";
import { KITTY_UNDER_BG_Z, imageLayers } from "./imageLayers";

function run(col: number, z: number): ImageRun {
  return {
    hash: "a".repeat(64),
    col,
    span: 1,
    u0: 0,
    v0: 0,
    u1: 1,
    v1: 1,
    padLeft: 0,
    padTop: 0,
    padRight: 0,
    padBottom: 0,
    z,
  };
}

const zs = (runs: ImageRun[]) => runs.map((r) => r.z);
const cols = (runs: ImageRun[]) => runs.map((r) => r.col);

describe("imageLayers", () => {
  it("exposes kitty's under-background border", () => {
    expect(KITTY_UNDER_BG_Z).toBe(-1073741824);
  });

  it("splits runs by z, with -1073741824 as the first under-text value", () => {
    const layers = imageLayers([
      run(0, -2000000000),
      run(1, -1073741825),
      run(2, -1073741824),
      run(3, -5),
      run(4, 0),
      run(5, 3),
    ]);

    expect(zs(layers.underBackground)).toEqual([-2000000000, -1073741825]);
    expect(zs(layers.underText)).toEqual([-1073741824, -5]);
    expect(zs(layers.overText)).toEqual([0, 3]);
  });

  it("sorts each layer by ascending z, so a higher z is drawn later", () => {
    const layers = imageLayers([run(0, 3), run(4, 1)]);

    expect(zs(layers.overText)).toEqual([1, 3]);
    expect(cols(layers.overText)).toEqual([4, 0]);
  });

  it("keeps row order for equal z", () => {
    const layers = imageLayers([run(0, 0), run(4, 0)]);

    expect(cols(layers.overText)).toEqual([0, 4]);
  });

  it("returns three empty layers for a row without runs", () => {
    expect(imageLayers([])).toEqual({ underBackground: [], underText: [], overText: [] });
  });
});
