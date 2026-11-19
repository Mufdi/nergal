import { describe, expect, it } from "vitest";
import { chipKeysActive } from "./gitChipKeys";

type Zone = "sidebar" | "terminal" | "panel" | "quake";

function opts(o: { inZen: boolean; anyZenOpen: boolean; zenZone?: string; focusZone?: Zone }) {
  return { zenZone: "main", focusZone: "panel" as Zone, ...o };
}

describe("chipKeysActive outside Zen", () => {
  it("is active when the git panel has focus and no Zen is open", () => {
    expect(chipKeysActive(opts({ inZen: false, anyZenOpen: false, focusZone: "panel" }))).toBe(true);
  });

  it.each<Zone>(["sidebar", "terminal", "quake"])("is inactive when focus is %s", (focusZone) => {
    expect(chipKeysActive(opts({ inZen: false, anyZenOpen: false, focusZone }))).toBe(false);
  });

  it("is inactive while a Zen is open even with panel focus", () => {
    expect(chipKeysActive(opts({ inZen: false, anyZenOpen: true, focusZone: "panel" }))).toBe(false);
  });
});

describe("chipKeysActive inside Zen", () => {
  it("is active when Zen is open and its zone is sidebar", () => {
    expect(chipKeysActive(opts({ inZen: true, anyZenOpen: true, zenZone: "sidebar" }))).toBe(true);
  });

  it("is inactive when the Zen zone is not sidebar", () => {
    expect(chipKeysActive(opts({ inZen: true, anyZenOpen: true, zenZone: "main" }))).toBe(false);
  });

  it("is inactive when no Zen is open", () => {
    expect(chipKeysActive(opts({ inZen: true, anyZenOpen: false, zenZone: "sidebar" }))).toBe(false);
  });

  it("ignores focusZone", () => {
    expect(
      chipKeysActive(opts({ inZen: true, anyZenOpen: true, zenZone: "sidebar", focusZone: "terminal" })),
    ).toBe(true);
  });
});
