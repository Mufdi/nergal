import { describe, it, expect } from "vitest";
import {
  indicatorDotClass,
  indicatorIcon,
  resolveIndicatorState,
  type IndicatorState,
} from "@/lib/sessionIndicatorState";

describe("indicatorIcon", () => {
  it.each<[IndicatorState, "check" | "x" | null]>([
    ["completed", "check"],
    ["error", "x"],
    ["idle", null],
    ["thinking", null],
    ["working", null],
    ["attention", null],
    ["done", null],
  ])("state %s renders %s", (state, expected) => {
    expect(indicatorIcon(state)).toBe(expected);
  });
});

describe("indicatorDotClass", () => {
  it.each<[IndicatorState, string]>([
    ["thinking", "bg-sky-400"],
    ["working", "bg-green-500"],
    ["attention", "bg-primary"],
    ["done", "ring-green-500"],
  ])("%s dot carries %s", (state, token) => {
    expect(indicatorDotClass(state)).toContain(token);
  });

  it("the status bar dot follows the resolved state, not the hook mode", () => {
    const state = resolveIndicatorState({
      sessionStatus: "running",
      mode: "idle",
      planReview: undefined,
      pendingAsk: false,
      pendingAttention: false,
      root: { state: "working", kind: null, msg: null },
    });
    expect(indicatorDotClass(state)).toContain("bg-green-500");
  });
});
