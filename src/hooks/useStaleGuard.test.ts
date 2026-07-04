import { describe, it, expect } from "vitest";
import { makeStaleChecker } from "@/hooks/useStaleGuard";

describe("makeStaleChecker", () => {
  it("reports fresh before a bump and stale after", () => {
    const checker = makeStaleChecker();
    const fresh = checker.capture();
    expect(fresh()).toBe(true);
    checker.bump();
    expect(fresh()).toBe(false);
  });

  it("keeps earlier captures independent of later ones", () => {
    const checker = makeStaleChecker();
    const first = checker.capture();
    checker.bump();
    const second = checker.capture();
    expect(first()).toBe(false);
    expect(second()).toBe(true);
    checker.bump();
    expect(first()).toBe(false);
    expect(second()).toBe(false);
  });

  it("does not go stale without a bump", () => {
    const checker = makeStaleChecker();
    const fresh = checker.capture();
    expect(fresh()).toBe(true);
    expect(fresh()).toBe(true);
  });
});
