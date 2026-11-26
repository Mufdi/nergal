import { describe, it, expect } from "vitest";
import {
  resolveIndicatorState,
  statusBarDot,
} from "@/lib/sessionIndicatorState";
import type { ProgramStatusEntry } from "@/stores/programStatus";

const base = {
  sessionStatus: "running",
  planReview: undefined,
  pendingAsk: false,
  pendingAttention: false,
};

describe("resolveIndicatorState with a root state the frontend does not know", () => {
  it.each([
    ["idle", "idle"],
    ["active", "thinking"],
    ["running", "working"],
  ])("falls back to the hook mode %s -> %s instead of throwing", (mode, expected) => {
    const root = { state: "paused", kind: null, msg: null } as unknown as ProgramStatusEntry;
    expect(resolveIndicatorState({ ...base, mode, root })).toBe(expected);
  });
});

describe("statusBarDot", () => {
  it("draws error as the X icon, not a color-only dot", () => {
    expect(statusBarDot("error").icon).toBe("x");
  });

  it("keeps a plain dot with its class for a state without an icon", () => {
    expect(statusBarDot("working")).toEqual({
      icon: null,
      className: "bg-green-500 text-green-500",
    });
  });
});
