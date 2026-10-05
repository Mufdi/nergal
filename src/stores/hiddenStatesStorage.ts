export const HIDDEN_STATES_KEY = "nergal.linear.hiddenStates";
export const COLLAPSED_GROUPS_KEY = "nergal.linear.collapsedGroups";

type ReadableStorage = Pick<Storage, "getItem">;
type WritableStorage = Pick<Storage, "setItem">;

export function serializeHiddenStates(states: ReadonlySet<string>): string {
  return JSON.stringify([...states]);
}

export function parseHiddenStates(raw: string | null): Set<string> {
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((v): v is string => typeof v === "string"));
  } catch {
    return new Set();
  }
}

export function loadStringSet(key: string, storage?: ReadableStorage): Set<string> {
  try {
    return parseHiddenStates((storage ?? localStorage).getItem(key));
  } catch {
    return new Set();
  }
}

export function saveStringSet(
  key: string,
  values: ReadonlySet<string>,
  storage?: WritableStorage,
): void {
  try {
    (storage ?? localStorage).setItem(key, serializeHiddenStates(values));
  } catch {
    // Storage blocked or full: the value just won't survive a restart.
  }
}

export function loadHiddenStates(storage?: ReadableStorage): Set<string> {
  return loadStringSet(HIDDEN_STATES_KEY, storage);
}

export function saveHiddenStates(
  states: ReadonlySet<string>,
  storage?: WritableStorage,
): void {
  saveStringSet(HIDDEN_STATES_KEY, states, storage);
}
