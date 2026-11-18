import type { ConfirmOptions } from "@/lib/confirm";
import { escapeHtml } from "@/lib/escapeHtml";
import { posixQuote } from "@/lib/shellQuote";

export interface LocalBranchRow {
  name: string;
  sha: string;
  pending: number | null;
  last_commit_unix: number;
  last_subject: string;
  session: { id: string; name: string } | null;
  in_use: boolean;
  missing_worktree: string | null;
}

export type LocalBranchDelete =
  | { kind: "deleted"; sha: string; repo: string }
  | { kind: "missing" }
  | { kind: "kept"; message: string };

export function branchActionsAllowed(row: LocalBranchRow): boolean {
  return row.session === null && !row.in_use;
}

export function orphanBranchCount(rows: LocalBranchRow[]): number {
  return rows.filter((r) => r.session === null).length;
}

export function deleteBranchConfirm(row: LocalBranchRow): ConfirmOptions {
  const count = row.pending === null ? "?" : String(row.pending);
  return {
    title: "Delete branch?",
    body: `<strong>${escapeHtml(row.name)}</strong> has ${count} commit(s) that exist only on this machine. They are gone for good once the branch is deleted, unless you restore it from the command shown afterwards.`,
    confirmLabel: "Delete",
    cancelLabel: "Cancel",
    kind: "warning",
    destructive: true,
  };
}

export function branchDeleteToast(
  branch: string,
  result: LocalBranchDelete,
): { message: string; description: string; type: "info" | "error" } {
  switch (result.kind) {
    case "deleted":
      return {
        message: "Branch deleted",
        description: `To restore it run: git -C ${posixQuote(result.repo)} branch ${branch} ${result.sha}`,
        type: "info",
      };
    case "missing":
      return { message: "Branch already deleted", description: branch, type: "info" };
    case "kept":
      return { message: "Branch kept", description: result.message, type: "error" };
  }
}
