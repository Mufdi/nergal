export const HIDDEN_STATES_KEY = "nergal.linear.hiddenStates";

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

export function loadHiddenStates(storage?: ReadableStorage): Set<string> {
  try {
    return parseHiddenStates(
      (storage ?? localStorage).getItem(HIDDEN_STATES_KEY),
    );
  } catch {
    return new Set();
  }
}

export function saveHiddenStates(
  states: ReadonlySet<string>,
  storage?: WritableStorage,
): void {
  try {
    (storage ?? localStorage).setItem(
      HIDDEN_STATES_KEY,
      serializeHiddenStates(states),
    );
  } catch {
    // Storage blocked or full: the filter just won't survive a restart.
  }
}
