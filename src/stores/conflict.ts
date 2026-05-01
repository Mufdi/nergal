import { atom } from "jotai";

export interface ConflictState {
  ours: string;
  theirs: string;
  merged: string;
  originalMerged: string;
  loaded: boolean;
}

export type ConflictKey = string;

export function conflictKey(sessionId: string, path: string): ConflictKey {
  return `${sessionId}\u0000${path}`;
}

export const conflictStateMapAtom = atom<Record<ConflictKey, ConflictState>>({});

export interface ZenConflictTarget {
  sessionId: string;
  path: string;
}

export const zenConflictTargetAtom = atom<ZenConflictTarget | null>(null);

/// Currently selected conflicted file inside the Conflicts chip (per session).
export const selectedConflictFileMapAtom = atom<Record<string, string | null>>({});

/// Per-session intent note for Ask Claude in conflict resolution.
export const conflictIntentMapAtom = atom<Record<ConflictKey, string>>({});

/// When true, Zen Mode renders the ConflictsPanel full-screen.
export const conflictsZenOpenAtom = atom(false);
