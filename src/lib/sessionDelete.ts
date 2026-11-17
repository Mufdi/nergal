import type { Session, Workspace } from "@/stores/workspace";

export function canDeleteSession(session: Session, workspace: Workspace): boolean {
  if (session.worktree_path !== null) return true;
  return workspace.sessions.some((s) => s.id !== session.id && s.worktree_path === null);
}
