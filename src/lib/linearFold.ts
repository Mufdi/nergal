import { isClosedState, normalizeStateName, type StateLike } from "./linearStateOrder";

export interface FoldContext {
  collapsed: ReadonlySet<string>;
  peeked: ReadonlyMap<string, StateLike>;
  hidden: ReadonlySet<string>;
  showCompleted: boolean;
}

export function foldedByStateFilter(
  state: StateLike | undefined,
  hidden: ReadonlySet<string>,
  showCompleted: boolean,
): boolean {
  const name = normalizeStateName(state?.stateName);
  if (!state || !name) return false;
  return hidden.has(name) || (!showCompleted && isClosedState(state));
}

// A filter-folded group ignores the persisted collapsed set: the filter is the
// persisted intent, and a peek only unfolds it until the filter changes.
export function isGroupFolded(key: string, state: StateLike | undefined, ctx: FoldContext): boolean {
  if (foldedByStateFilter(state, ctx.hidden, ctx.showCompleted)) return !ctx.peeked.has(key);
  return ctx.collapsed.has(key);
}

export function withoutPeeks(
  peeked: ReadonlyMap<string, StateLike>,
  drop: (state: StateLike) => boolean,
): ReadonlyMap<string, StateLike> {
  let next: Map<string, StateLike> | null = null;
  for (const [key, state] of peeked) {
    if (!drop(state)) continue;
    next ??= new Map(peeked);
    next.delete(key);
  }
  return next ?? peeked;
}
