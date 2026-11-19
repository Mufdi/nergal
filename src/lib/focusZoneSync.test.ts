import { describe, expect, it } from "vitest";
import { zoneOf } from "./focusZoneSync";

function fakeEl(zones: string[]) {
  return {
    closest(selector: string) {
      return zones.some((z) => selector === `[data-focus-zone='${z}']`) ? {} : null;
    },
  };
}

describe("zoneOf", () => {
  it("returns null for a null element", () => {
    expect(zoneOf(null)).toBeNull();
  });

  it("returns null for an element outside every zone", () => {
    expect(zoneOf(fakeEl([]))).toBeNull();
  });

  it.each(["quake", "panel", "sidebar", "terminal"] as const)("returns %s", (zone) => {
    expect(zoneOf(fakeEl([zone]))).toBe(zone);
  });

  it("prefers panel over terminal when nested", () => {
    expect(zoneOf(fakeEl(["panel", "terminal"]))).toBe("panel");
  });

  it("prefers quake over terminal when nested", () => {
    expect(zoneOf(fakeEl(["quake", "terminal"]))).toBe("quake");
  });

  it("prefers panel over sidebar and sidebar over terminal", () => {
    expect(zoneOf(fakeEl(["panel", "sidebar"]))).toBe("panel");
    expect(zoneOf(fakeEl(["sidebar", "terminal"]))).toBe("sidebar");
  });
});
