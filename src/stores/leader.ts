import { atom } from "jotai";
import { appStore } from "./jotaiStore";
import { parseChord, type ParsedShortcut } from "@/lib/keymap";
import type { ShortcutAction } from "./shortcuts";

/// Time the leader stays pending (awaiting a continuation, or armed in raw
/// mode) before auto-cancelling. Matches OpenCode's leader-key timeout (D1).
export const LEADER_TIMEOUT_MS = 2000;

export interface LeaderPending {
  mode: "awaiting" | "raw";
  /// True once a non-continuation Ctrl/Alt combo or plain key has been
  /// pressed while awaiting — the which-key popover (Phase 4) swaps its
  /// normal listing for the "send to terminal" hint pointing at `.`.
  hintNonContinuation: boolean;
}

/// Phase 4 (StatusBar breadcrumb + which-key popover) consumes this shape
/// directly — keep it stable.
export const leaderPendingAtom = atom<LeaderPending | null>(null);

let deadlineTimer: ReturnType<typeof setTimeout> | null = null;

function clearDeadline(): void {
  if (deadlineTimer !== null) {
    clearTimeout(deadlineTimer);
    deadlineTimer = null;
  }
}

function armDeadline(): void {
  clearDeadline();
  deadlineTimer = setTimeout(() => {
    deadlineTimer = null;
    appStore.set(leaderPendingAtom, null);
  }, LEADER_TIMEOUT_MS);
}

export function startLeaderPending(): void {
  appStore.set(leaderPendingAtom, { mode: "awaiting", hintNonContinuation: false });
  armDeadline();
}

export function enterRawMode(): void {
  appStore.set(leaderPendingAtom, { mode: "raw", hintNonContinuation: false });
  armDeadline();
}

/// A non-continuation keydown while awaiting is a safe no-op, not a cancel —
/// resets the deadline so the user has the full window to read the hint.
export function flagNonContinuationHint(): void {
  const current = appStore.get(leaderPendingAtom);
  if (!current) return;
  appStore.set(leaderPendingAtom, { ...current, hintNonContinuation: true });
  armDeadline();
}

export function cancelLeaderPending(): void {
  clearDeadline();
  appStore.set(leaderPendingAtom, null);
}

export const BARE_MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight",
  "ShiftLeft", "ShiftRight",
  "AltLeft", "AltRight",
  "MetaLeft", "MetaRight",
]);

interface KeyLike {
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export type ContinuationResolution =
  | { kind: "continuation"; action: ShortcutAction }
  | { kind: "raw" }
  | { kind: "cancel" }
  | { kind: "hint" }
  | { kind: "ignore" };

/// Pure resolution for a keydown while the leader is `awaiting` a
/// continuation (D2 passthrough semantics) — no DOM reads, so it's
/// unit-testable in isolation. The dialog-open self-cancel check and the
/// actual atom mutation stay in the dispatcher; this only classifies the
/// event.
export function resolveContinuation(
  e: KeyLike,
  leaderBinding: ParsedShortcut,
  chordEntries: ShortcutAction[],
): ContinuationResolution {
  if (BARE_MODIFIER_CODES.has(e.code)) return { kind: "ignore" };
  if (e.code === "Escape") return { kind: "cancel" };
  if (
    e.code === leaderBinding.code &&
    e.ctrlKey === leaderBinding.ctrl &&
    e.shiftKey === leaderBinding.shift &&
    e.altKey === leaderBinding.alt
  ) {
    return { kind: "cancel" };
  }
  if (e.code === "Period" && !e.ctrlKey && !e.altKey && !e.shiftKey) return { kind: "raw" };
  // Sloppy-chording tolerance (D2): Ctrl held over from the leader press
  // still resolves to the continuation; Alt never does — an Alt combo falls
  // through to the safe-no-op hint below.
  if (!e.altKey) {
    for (const entry of chordEntries) {
      const parsed = parseChord(entry.keys);
      if (!parsed) continue;
      if (e.code === parsed.code && e.shiftKey === parsed.shift) {
        return { kind: "continuation", action: entry };
      }
    }
  }
  return { kind: "hint" };
}
