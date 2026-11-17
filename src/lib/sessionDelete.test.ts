import { describe, expect, it } from "vitest";
import { canDeleteSession } from "./sessionDelete";
import type { Session, Workspace } from "@/stores/workspace";

function session(id: string, worktreePath: string | null): Session {
  return {
    id,
    name: id,
    workspace_id: "w",
    worktree_path: worktreePath,
    worktree_branch: worktreePath ? "feat/x" : null,
    merge_target: null,
    status: "idle",
    created_at: 100,
    updated_at: 100,
  };
}

function workspace(sessions: Session[]): Workspace {
  return { id: "w", name: "w", repo_path: "/repo", sessions, created_at: 1, is_git: true };
}

describe("canDeleteSession", () => {
  it("hides delete on the only repo-root session of a workspace", () => {
    const r1 = session("r1", null);
    const t1 = session("t1", "/repo/.worktrees/t1");
    expect(canDeleteSession(r1, workspace([r1, t1]))).toBe(false);
  });

  it("offers delete on a worktree session", () => {
    const r1 = session("r1", null);
    const t1 = session("t1", "/repo/.worktrees/t1");
    expect(canDeleteSession(t1, workspace([r1, t1]))).toBe(true);
  });

  it("offers delete on both rows when the workspace has two repo-root sessions", () => {
    const r1 = session("r1", null);
    const r2 = session("r2", null);
    const ws = workspace([r1, r2]);
    expect(canDeleteSession(r1, ws)).toBe(true);
    expect(canDeleteSession(r2, ws)).toBe(true);
  });

  it("hides delete on a repo-root session that is alone in its workspace", () => {
    const r1 = session("r1", null);
    expect(canDeleteSession(r1, workspace([r1]))).toBe(false);
  });
});
