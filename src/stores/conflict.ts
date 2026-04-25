import { atom } from "jotai";
import { openTabAction, expandRightPanelAtom, activePanelViewAtom } from "./rightPanel";

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

/// Currently selected conflicted file inside the Conflicts tab (per session).
export const selectedConflictFileMapAtom = atom<Record<string, string | null>>({});

/// Per-session intent note for Ask Claude in conflict resolution.
export const conflictIntentMapAtom = atom<Record<ConflictKey, string>>({});

/// Opens (or focuses) the singleton Conflicts tab and selects the given file.
/// Also expands the right panel if collapsed so the keyboard shortcut behaves
/// like the other `togglePanel` shortcuts (plan / files / diff / spec / git).
export const openConflictsTabAction = atom(
  null,
  (_get, set, params: { sessionId: string; path?: string }) => {
    set(openTabAction, { tab: { id: "conflicts", type: "conflicts", label: "Conflicts", data: {} }, isPinned: true });
    if (params.path) {
      set(selectedConflictFileMapAtom, (prev) => ({ ...prev, [params.sessionId]: params.path! }));
    }
    set(activePanelViewAtom, "conflicts");
    set(expandRightPanelAtom, (prev: number) => prev + 1);
  },
);

/// When true, Zen Mode renders the ConflictsPanel full-screen.
export const conflictsZenOpenAtom = atom(false);
