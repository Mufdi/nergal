import { describe, expect, it } from "vitest";
import { pushBounded, withBoundedEntry } from "./boundedRecord";

describe("withBoundedEntry", () => {
  it("evicts the least recently written keys past the cap", () => {
    let r: Record<string, number> = {};
    for (const k of ["a", "b", "c", "d"]) r = withBoundedEntry(r, k, 1, 3);
    expect(Object.keys(r)).toEqual(["b", "c", "d"]);
  });

  it("rewriting a key refreshes its recency", () => {
    let r: Record<string, number> = { a: 1, b: 2, c: 3 };
    r = withBoundedEntry(r, "a", 9, 3);
    r = withBoundedEntry(r, "d", 4, 3);
    expect(r).toEqual({ c: 3, a: 9, d: 4 });
  });
});

describe("pushBounded", () => {
  it("keeps only the newest items", () => {
    expect(pushBounded([1, 2, 3], 4, 3)).toEqual([2, 3, 4]);
  });
});
