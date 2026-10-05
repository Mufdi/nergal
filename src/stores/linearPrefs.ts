export const MAX_COLLAPSED_GROUPS = 500;

// Group keys embed server ids, so keys of deleted projects/states would
// otherwise accumulate in the config file forever. Set preserves insertion
// order, so the tail holds the most recently folded keys.
export function boundRecent(values: ReadonlySet<string>, max: number): Set<string> {
  const all = [...values];
  return new Set(all.length > max ? all.slice(all.length - max) : all);
}
