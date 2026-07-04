import { atom, type Getter, type PrimitiveAtom, type Setter } from "jotai";
import { sessionScopedMapAtom } from "./sessionScope";

export interface ConflictState {
  ours: string;
  theirs: string;
  merged: string;
  originalMerged: string;
  loaded: boolean;
}

export type ConflictKey = string;

// NUL is never a legal path/sessionId character, so it's a safe join separator.
// Built via fromCharCode (not a literal escape) to keep this source file plain ASCII.
const CONFLICT_KEY_SEP = String.fromCharCode(0);

export function conflictKey(sessionId: string, path: string): ConflictKey {
  return sessionId + CONFLICT_KEY_SEP + path;
}

export const conflictStateMapAtom = atom<Record<ConflictKey, ConflictState>>({});

export interface ZenConflictTarget {
  sessionId: string;
  path: string;
}

export const zenConflictTargetAtom = atom<ZenConflictTarget | null>(null);

/// Currently selected conflicted file inside the Conflicts chip (per session).
export const selectedConflictFileMapAtom = sessionScopedMapAtom<string | null>();

/// Per-session intent note for Ask Claude in conflict resolution.
export const conflictIntentMapAtom = atom<Record<ConflictKey, string>>({});

/// When true, Zen Mode renders the ConflictsPanel full-screen.
export const conflictsZenOpenAtom = atom(false);

function pruneByConflictPrefix<T>(
  get: Getter,
  set: Setter,
  mapAtom: PrimitiveAtom<Record<ConflictKey, T>>,
  prefix: string,
): void {
  const current = get(mapAtom);
  const keys = Object.keys(current).filter((k) => k.startsWith(prefix));
  if (keys.length === 0) return;
  const next = { ...current };
  for (const k of keys) delete next[k];
  set(mapAtom, next);
}

/// `conflictStateMapAtom` / `conflictIntentMapAtom` are keyed by the composite
/// `conflictKey(sessionId, path)`, so they can't go through the sessionId-only
/// registry in `sessionScope.ts` — pruned ad-hoc here, called from the same
/// permanent-close sites as `pruneSessionStateAction`.
export const pruneConflictSessionAction = atom(null, (get, set, sessionId: string) => {
  const prefix = sessionId + CONFLICT_KEY_SEP;
  pruneByConflictPrefix(get, set, conflictStateMapAtom, prefix);
  pruneByConflictPrefix(get, set, conflictIntentMapAtom, prefix);
});
