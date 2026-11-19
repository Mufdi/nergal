export interface StateLike {
  stateName?: string;
  stateType?: string;
  statePosition?: number;
}

export interface StateFilterOption {
  key: string;
  name: string;
  stateType: string | null;
  color: string | null;
}

const CLOSED_FROM = 10;
const NO_STATE_RANK = 20;
const UNKNOWN_OPEN_RANK = 5;

// From the state type only, so renaming or adding a state in Linear needs no
// change here.
const RANK_BY_TYPE: Record<string, number> = {
  started: 1,
  unstarted: 2,
  backlog: 3,
  triage: 4,
  completed: 10,
  duplicate: 11,
  canceled: 12,
  cancelled: 12,
};

export function normalizeStateName(name: string | undefined): string {
  return (name ?? "").trim().toLowerCase();
}

export function stateRank(s: StateLike): number {
  if (s.stateType == null) return s.stateName ? UNKNOWN_OPEN_RANK : NO_STATE_RANK;
  return RANK_BY_TYPE[s.stateType] ?? UNKNOWN_OPEN_RANK;
}

export function isClosedState(s: StateLike): boolean {
  const rank = stateRank(s);
  return rank >= CLOSED_FROM && rank < NO_STATE_RANK;
}

// Linear sorts by status "closest to done first": an open workflow lists its
// later columns first, a closed one keeps its own order.
export function compareStates(a: StateLike, b: StateLike): number {
  const ra = stateRank(a);
  const rb = stateRank(b);
  if (ra !== rb) return ra - rb;
  const pa = a.statePosition ?? 0;
  const pb = b.statePosition ?? 0;
  if (pa !== pb) return ra >= CLOSED_FROM ? pa - pb : pb - pa;
  return normalizeStateName(a.stateName).localeCompare(normalizeStateName(b.stateName));
}

export function stateFilterOptions(
  issues: readonly (StateLike & { stateColor?: string })[],
): StateFilterOption[] {
  const byKey = new Map<string, { option: StateFilterOption; sample: StateLike }>();
  for (const i of issues) {
    const key = normalizeStateName(i.stateName);
    if (!key) continue;
    const seen = byKey.get(key);
    if (seen) {
      if (compareStates(i, seen.sample) < 0) seen.sample = i;
      continue;
    }
    byKey.set(key, {
      option: {
        key,
        name: (i.stateName ?? "").trim(),
        stateType: i.stateType ?? null,
        color: i.stateColor ?? null,
      },
      sample: i,
    });
  }
  return [...byKey.values()]
    .sort((a, b) => compareStates(a.sample, b.sample))
    .map((e) => e.option);
}

export interface StateSubGroup<T> {
  key: string;
  label: string;
  stateType: string | null;
  color: string | null;
  issues: T[];
  state: StateLike;
}

export function groupByState<T extends StateLike & { stateColor?: string }>(
  issues: readonly T[],
): StateSubGroup<T>[] {
  const groups = new Map<string, StateSubGroup<T>>();
  for (const i of issues) {
    const key = normalizeStateName(i.stateName);
    let g = groups.get(key);
    if (!g) {
      g = {
        key,
        label: key ? (i.stateName ?? "").trim() : "No state",
        stateType: i.stateType ?? null,
        color: i.stateColor ?? null,
        issues: [],
        state: i,
      };
      groups.set(key, g);
    } else if (compareStates(i, g.state) < 0) {
      // Two teams can share a state name with different positions; the best
      // member ranks the group so the order does not follow arrival order.
      g.state = i;
    }
    g.issues.push(i);
  }
  return [...groups.values()].sort((a, b) => compareStates(a.state, b.state));
}
