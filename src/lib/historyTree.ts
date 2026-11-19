export type HistoryTreeItem =
  | { kind: "commit"; key: string; hash: string }
  | { kind: "file"; key: string; hash: string; path: string };

export type HistoryTreeAction =
  | { type: "toggle-commit"; hash: string }
  | { type: "open-file"; hash: string; path: string };

export function historyTreeItems(
  commits: { hash: string }[],
  expandedCommit: string | null,
  commitFiles: Record<string, string[]>,
): HistoryTreeItem[] {
  const items: HistoryTreeItem[] = [];
  for (const c of commits) {
    items.push({ kind: "commit", key: `c:${c.hash}`, hash: c.hash });
    if (expandedCommit !== c.hash) continue;
    for (const path of commitFiles[c.hash] ?? []) {
      items.push({ kind: "file", key: `f:${c.hash}:${path}`, hash: c.hash, path });
    }
  }
  return items;
}

export function historyTreeKeyAction(item: HistoryTreeItem | undefined, code: string): HistoryTreeAction | null {
  if (!item) return null;
  if (item.kind === "commit") {
    return code === "Space" || code === "Enter" ? { type: "toggle-commit", hash: item.hash } : null;
  }
  return code === "Enter" ? { type: "open-file", hash: item.hash, path: item.path } : null;
}
