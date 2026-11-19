import type { ConfirmOptions } from "@/lib/confirm";
import { escapeHtml } from "@/lib/escapeHtml";
import { branchActionsAllowed, type LocalBranchRow } from "@/lib/localBranches";

export interface BranchCommit {
  hash: string;
  message: string;
}

export type BranchTreeItem =
  | { kind: "branch"; key: string; row: LocalBranchRow }
  | { kind: "commit"; key: string; branch: string; hash: string; message: string }
  | { kind: "file"; key: string; branch: string; hash: string; path: string };

export interface BranchTreeState {
  expandedBranch: string | null;
  expandedCommit: string | null;
  branchCommits: Record<string, BranchCommit[]>;
  commitFiles: Record<string, string[]>;
}

export type BranchTreeAction =
  | { type: "toggle-branch"; branch: string }
  | { type: "toggle-commit"; hash: string }
  | { type: "open-file"; hash: string; path: string }
  | { type: "open-session"; row: LocalBranchRow }
  | { type: "delete"; row: LocalBranchRow };

export function branchKey(name: string): string {
  return `b:${name}`;
}

export function branchTreeItems(rows: LocalBranchRow[], s: BranchTreeState): BranchTreeItem[] {
  const items: BranchTreeItem[] = [];
  for (const row of rows) {
    items.push({ kind: "branch", key: branchKey(row.name), row });
    if (s.expandedBranch !== row.name) continue;
    for (const c of s.branchCommits[row.name] ?? []) {
      items.push({ kind: "commit", key: `c:${row.name}:${c.hash}`, branch: row.name, hash: c.hash, message: c.message });
      if (s.expandedCommit !== c.hash) continue;
      for (const path of s.commitFiles[c.hash] ?? []) {
        items.push({ kind: "file", key: `f:${row.name}:${c.hash}:${path}`, branch: row.name, hash: c.hash, path });
      }
    }
  }
  return items;
}

export function cursorIndexFor(items: { key: string }[], key: string | null, fallback: number): number {
  if (items.length === 0) return 0;
  if (key !== null) {
    const idx = items.findIndex((i) => i.key === key);
    if (idx !== -1) return idx;
  }
  return Math.min(Math.max(fallback, 0), items.length - 1);
}

export function branchTreeKeyAction(item: BranchTreeItem | undefined, code: string): BranchTreeAction | null {
  if (!item) return null;
  switch (item.kind) {
    case "branch":
      if (code === "Space" || code === "Enter") return { type: "toggle-branch", branch: item.row.name };
      if (!branchActionsAllowed(item.row)) return null;
      if (code === "KeyO") return { type: "open-session", row: item.row };
      if (code === "KeyD") return { type: "delete", row: item.row };
      return null;
    case "commit":
      if (code === "Space" || code === "Enter") return { type: "toggle-commit", hash: item.hash };
      return null;
    case "file":
      if (code === "Enter") return { type: "open-file", hash: item.hash, path: item.path };
      return null;
  }
}

export function changedBranches(prev: LocalBranchRow[], next: LocalBranchRow[]): string[] {
  const nextSha = new Map(next.map((r) => [r.name, r.sha]));
  return prev.filter((r) => nextSha.get(r.name) !== r.sha).map((r) => r.name);
}

export function openSessionConfirm(row: LocalBranchRow): ConfirmOptions {
  return {
    title: "Open session?",
    body: `Creates a worktree on <strong>${escapeHtml(row.name)}</strong> and starts a new Claude Code session there.`,
    confirmLabel: "Open session",
    cancelLabel: "Cancel",
    kind: "question",
    destructive: false,
  };
}
