import { describe, it, expect } from "vitest";
import { isAwaitingSession, resolveIndicatorState } from "@/lib/sessionIndicatorState";
import type { ProgramStatusEntry, RootState } from "@/stores/programStatus";

function root(state: RootState): ProgramStatusEntry {
  return { state, kind: null, msg: null };
}

const base = {
  sessionStatus: "running" as const,
  mode: "idle",
  planReview: undefined as string | undefined,
  pendingAsk: false,
  pendingAttention: false,
  root: undefined as ProgramStatusEntry | undefined,
};

describe("resolveIndicatorState without a root status", () => {
  it.each([
    ["idle", "idle"],
    ["active", "thinking"],
    ["Read", "working"],
    ["Bash", "working"],
  ])("mode %s resolves to %s", (mode, expected) => {
    expect(resolveIndicatorState({ ...base, mode })).toBe(expected);
  });
});

describe("resolveIndicatorState attention", () => {
  it.each([
    ["plan review pending_review", { planReview: "pending_review" }],
    ["pending ask", { pendingAsk: true }],
    ["pending notification attention", { pendingAttention: true }],
    ["blocked root without kind", { root: root("blocked") }],
  ])("%s wins over the mode", (_name, extra) => {
    expect(resolveIndicatorState({ ...base, mode: "Read", ...extra })).toBe("attention");
  });

  it("hook attention wins over a done root", () => {
    expect(resolveIndicatorState({ ...base, pendingAsk: true, root: root("done") })).toBe("attention");
  });

  it("hook attention wins over a completed session", () => {
    expect(
      resolveIndicatorState({ ...base, sessionStatus: "completed", pendingAttention: true }),
    ).toBe("attention");
  });

  it("a plan review that is not pending does not raise attention", () => {
    expect(resolveIndicatorState({ ...base, planReview: "submitted" })).toBe("idle");
  });
});

describe("resolveIndicatorState with a root status", () => {
  it("working root after Stop (mode idle) is working", () => {
    expect(resolveIndicatorState({ ...base, mode: "idle", root: root("working") })).toBe("working");
  });

  it("working root with mode active is thinking", () => {
    expect(resolveIndicatorState({ ...base, mode: "active", root: root("working") })).toBe("thinking");
  });

  it("working root with a tool name is working", () => {
    expect(resolveIndicatorState({ ...base, mode: "Edit", root: root("working") })).toBe("working");
  });

  it("done root wins over a late tool mode", () => {
    expect(resolveIndicatorState({ ...base, mode: "Read", root: root("done") })).toBe("done");
  });

  it("error root is error", () => {
    expect(resolveIndicatorState({ ...base, mode: "Read", root: root("error") })).toBe("error");
  });

  it("idle root wins over mode active", () => {
    expect(resolveIndicatorState({ ...base, mode: "active", root: root("idle") })).toBe("idle");
  });

  it("completed session stays completed even with a root", () => {
    expect(
      resolveIndicatorState({ ...base, sessionStatus: "completed", mode: "Read", root: root("working") }),
    ).toBe("completed");
  });

  it("a removed root falls back to the hook mode", () => {
    expect(resolveIndicatorState({ ...base, mode: "Bash", root: undefined })).toBe("working");
  });
});

describe("isAwaitingSession", () => {
  it.each([
    ["pending ask", { pendingAsk: true }],
    ["pending notification attention", { pendingAttention: true }],
    ["blocked root", { root: root("blocked") }],
  ])("is true for %s", (_name, extra) => {
    expect(isAwaitingSession({ pendingAsk: false, pendingAttention: false, root: undefined, ...extra })).toBe(true);
  });

  it.each([
    ["working root", root("working")],
    ["no root", undefined],
  ])("is false for %s with no hook attention", (_name, entry) => {
    expect(isAwaitingSession({ pendingAsk: false, pendingAttention: false, root: entry })).toBe(false);
  });
});
