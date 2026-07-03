import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  leaderPendingAtom,
  startLeaderPending,
  enterRawMode,
  flagNonContinuationHint,
  cancelLeaderPending,
  resolveContinuation,
  LEADER_TIMEOUT_MS,
} from "@/stores/leader";
import { appStore } from "@/stores/jotaiStore";
import type { ShortcutAction } from "@/stores/shortcuts";

const LEADER_BINDING = { ctrl: true, shift: false, alt: false, code: "Space" };

function chord(id: string, keys: string): ShortcutAction {
  return { id, label: id, keys, category: "action", keywords: [], handler: vi.fn() };
}

function key(overrides: Partial<{ code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }>) {
  return { code: "", ctrlKey: false, shiftKey: false, altKey: false, ...overrides };
}

describe("leader pending lifecycle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    cancelLeaderPending();
  });

  afterEach(() => {
    cancelLeaderPending();
    vi.useRealTimers();
  });

  it("startLeaderPending enters awaiting mode", () => {
    startLeaderPending();
    expect(appStore.get(leaderPendingAtom)).toEqual({ mode: "awaiting", hintNonContinuation: false });
  });

  it("enterRawMode enters raw mode", () => {
    enterRawMode();
    expect(appStore.get(leaderPendingAtom)).toEqual({ mode: "raw", hintNonContinuation: false });
  });

  it("cancelLeaderPending clears state", () => {
    startLeaderPending();
    cancelLeaderPending();
    expect(appStore.get(leaderPendingAtom)).toBeNull();
  });

  it("times out after LEADER_TIMEOUT_MS", () => {
    startLeaderPending();
    vi.advanceTimersByTime(LEADER_TIMEOUT_MS);
    expect(appStore.get(leaderPendingAtom)).toBeNull();
  });

  it("flagNonContinuationHint sets the hint and resets the deadline", () => {
    startLeaderPending();
    vi.advanceTimersByTime(LEADER_TIMEOUT_MS - 100);
    flagNonContinuationHint();
    expect(appStore.get(leaderPendingAtom)).toEqual({ mode: "awaiting", hintNonContinuation: true });
    // Deadline reset — the original timeout should NOT fire.
    vi.advanceTimersByTime(150);
    expect(appStore.get(leaderPendingAtom)).not.toBeNull();
    vi.advanceTimersByTime(LEADER_TIMEOUT_MS);
    expect(appStore.get(leaderPendingAtom)).toBeNull();
  });

  it("flagNonContinuationHint is a no-op when not pending", () => {
    flagNonContinuationHint();
    expect(appStore.get(leaderPendingAtom)).toBeNull();
  });

  it("a later startLeaderPending clears a stale deadline", () => {
    startLeaderPending();
    vi.advanceTimersByTime(LEADER_TIMEOUT_MS - 100);
    startLeaderPending();
    vi.advanceTimersByTime(150);
    expect(appStore.get(leaderPendingAtom)).not.toBeNull();
  });
});

describe("resolveContinuation", () => {
  const chords = [chord("new-session", "leader n"), chord("add-workspace", "leader shift+w"), chord("zen", "leader 0")];

  it("ignores bare modifier keydowns", () => {
    expect(resolveContinuation(key({ code: "ControlLeft" }), LEADER_BINDING, chords)).toEqual({ kind: "ignore" });
  });

  it("cancels on Esc", () => {
    expect(resolveContinuation(key({ code: "Escape" }), LEADER_BINDING, chords)).toEqual({ kind: "cancel" });
  });

  it("cancels on a leader re-tap", () => {
    expect(resolveContinuation(key({ code: "Space", ctrlKey: true }), LEADER_BINDING, chords)).toEqual({ kind: "cancel" });
  });

  it("enters raw mode on a plain period", () => {
    expect(resolveContinuation(key({ code: "Period" }), LEADER_BINDING, chords)).toEqual({ kind: "raw" });
  });

  it("does not enter raw mode when Ctrl is held with period", () => {
    const result = resolveContinuation(key({ code: "Period", ctrlKey: true }), LEADER_BINDING, chords);
    expect(result.kind).not.toBe("raw");
  });

  it("resolves a plain continuation", () => {
    const result = resolveContinuation(key({ code: "KeyN" }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "continuation", action: chords[0] });
  });

  it("tolerates sloppy chording (Ctrl still held)", () => {
    const result = resolveContinuation(key({ code: "KeyN", ctrlKey: true }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "continuation", action: chords[0] });
  });

  it("resolves a shift continuation distinctly from its plain form", () => {
    const result = resolveContinuation(key({ code: "KeyW", shiftKey: true }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "continuation", action: chords[1] });
  });

  it("does not match a shift continuation without shift held", () => {
    const result = resolveContinuation(key({ code: "KeyW" }), LEADER_BINDING, chords);
    expect(result.kind).toBe("hint");
  });

  it("hints on a non-continuation Ctrl combo instead of forwarding", () => {
    const result = resolveContinuation(key({ code: "KeyZ", ctrlKey: true }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "hint" });
  });

  it("hints on an Alt combo even if the letter is a continuation", () => {
    const result = resolveContinuation(key({ code: "KeyN", altKey: true }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "hint" });
  });

  it("hints on any other plain non-continuation key", () => {
    const result = resolveContinuation(key({ code: "KeyJ" }), LEADER_BINDING, chords);
    expect(result).toEqual({ kind: "hint" });
  });
});
