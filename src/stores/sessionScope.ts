import { atom, type PrimitiveAtom } from "jotai";

/// Every `Record<sessionId, T>` atom in the stores MUST be created through
/// `sessionScopedMapAtom`, never via a bare `atom<Record<string, T>>({})`.
/// Registering at the definition site is what lets `pruneSessionStateAction`
/// reclaim every session-keyed map without a hand-maintained list — a manual
/// list rots the moment someone adds a new map and forgets to wire it in.
interface RegistryEntry {
  atom: PrimitiveAtom<Record<string, unknown>>;
  onPrune?: (value: unknown) => void;
}

const registry: RegistryEntry[] = [];

export function sessionScopedMapAtom<T>(
  initial: Record<string, T> = {},
  onPrune?: (value: T) => void,
): PrimitiveAtom<Record<string, T>> {
  const mapAtom = atom(initial);
  registry.push({
    atom: mapAtom as PrimitiveAtom<Record<string, unknown>>,
    onPrune: onPrune as ((value: unknown) => void) | undefined,
  });
  return mapAtom;
}

/// Reclaims one session's entry from every registered map. Fires only at
/// permanent-close time (finalize / grace-delete) — never at soft-close,
/// which must keep state alive for the undo window (design D2).
export const pruneSessionStateAction = atom(null, (get, set, sessionId: string) => {
  for (const { atom: mapAtom, onPrune } of registry) {
    const current = get(mapAtom);
    if (!(sessionId in current)) continue;
    onPrune?.(current[sessionId]);
    const { [sessionId]: _dropped, ...rest } = current;
    set(mapAtom, rest);
  }
});
