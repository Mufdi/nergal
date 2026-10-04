/// Insert `key` as the most recent entry and drop the oldest ones past `max`.
/// Relies on string-key insertion order, so a rewrite deletes first to move
/// the key to the end.
export function withBoundedEntry<V>(
  record: Record<string, V>,
  key: string,
  value: V,
  max: number,
): Record<string, V> {
  const next = { ...record };
  delete next[key];
  next[key] = value;
  const keys = Object.keys(next);
  for (const stale of keys.slice(0, Math.max(0, keys.length - max))) delete next[stale];
  return next;
}

export function pushBounded<T>(list: T[], item: T, max: number): T[] {
  return [...list, item].slice(-max);
}
