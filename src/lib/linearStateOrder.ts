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

// Name wins over type: Linear gives Duplicate and Canceled the same `canceled`
// type, and team RR's legacy Pending is `completed` but means In Review.
const RANK_BY_NAME: Record<string, number> = {
  "in review": 0,
  pending: 0,
  "in progress": 1,
  todo: 2,
  backlog: 3,
  triage: 4,
  done: 10,
  duplicate: 11,
  canceled: 12,
  cancelled: 12,
};

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
  const byName = RANK_BY_NAME[normalizeStateName(s.stateName)];
  if (byName !== undefined) return byName;
  if (s.stateType == null) return s.stateName ? UNKNOWN_OPEN_RANK : NO_STATE_RANK;
  return RANK_BY_TYPE[s.stateType] ?? UNKNOWN_OPEN_RANK;
}

export function isClosedState(s: StateLike): boolean {
  const rank = stateRank(s);
  return rank >= CLOSED_FROM && rank < NO_STATE_RANK;
}

export function compareStates(a: StateLike, b: StateLike): number {
  const ra = stateRank(a);
  const rb = stateRank(b);
  if (ra !== rb) return ra - rb;
  return (a.statePosition ?? 0) - (b.statePosition ?? 0);
}

export function filterHiddenStates<T extends StateLike>(
  issues: T[],
  hidden: ReadonlySet<string>,
): T[] {
  if (hidden.size === 0) return issues;
  return issues.filter((i) => !hidden.has(normalizeStateName(i.stateName)));
}

export function stateFilterOptions(
  issues: readonly (StateLike & { stateColor?: string })[],
): StateFilterOption[] {
  const byKey = new Map<string, { option: StateFilterOption; sample: StateLike }>();
  for (const i of issues) {
    const key = normalizeStateName(i.stateName);
    if (!key || byKey.has(key)) continue;
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
    }
    g.issues.push(i);
  }
  return [...groups.values()].sort((a, b) => compareStates(a.state, b.state));
}
