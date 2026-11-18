import { describe, expect, it } from "vitest";
import { MAX_COLLAPSED_GROUPS, boundRecent } from "./linearPrefs";

describe("boundRecent", () => {
  it("keeps everything under the cap", () => {
    expect([...boundRecent(new Set(["a", "b"]), 5)]).toEqual(["a", "b"]);
  });

  it("drops the oldest entries past the cap", () => {
    expect([...boundRecent(new Set(["a", "b", "c", "d"]), 2)]).toEqual(["c", "d"]);
  });

  it("caps collapsed groups at 500", () => {
    const big = new Set(Array.from({ length: 600 }, (_, i) => `k${i}`));
    const out = boundRecent(big, MAX_COLLAPSED_GROUPS);
    expect(out.size).toBe(500);
    expect(out.has("k599")).toBe(true);
    expect(out.has("k99")).toBe(false);
  });
});
