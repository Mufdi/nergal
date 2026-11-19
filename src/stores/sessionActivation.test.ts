import { beforeEach, describe, expect, it } from "vitest";
import { createStore } from "jotai";
import { addAndActivateSessionAtom } from "./sessionActivation";
import {
  activeSessionIdAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
  type Workspace,
} from "./workspace";

function makeSession(id: string, workspaceId: string): Session {
  return {
    id,
    name: id,
    workspace_id: workspaceId,
    worktree_path: null,
    worktree_branch: null,
    merge_target: null,
    status: "idle",
    created_at: 1,
    updated_at: 1,
  };
}

function makeWorkspace(id: string, sessions: Session[] = []): Workspace {
  return {
    id,
    name: id,
    repo_path: `/repo/${id}`,
    sessions,
    created_at: 1,
    is_git: true,
  } as Workspace;
}

describe("addAndActivateSessionAtom", () => {
  let store: ReturnType<typeof createStore>;

  beforeEach(() => {
    store = createStore();
    store.set(workspacesAtom, [
      makeWorkspace("w1", [makeSession("existing", "w1")]),
      makeWorkspace("w2"),
    ]);
  });

  it("appends the session to its workspace once, even when added twice", () => {
    const session = makeSession("s1", "w1");
    store.set(addAndActivateSessionAtom, session);
    store.set(addAndActivateSessionAtom, session);
    const [w1, w2] = store.get(workspacesAtom);
    expect(w1.sessions.map((s) => s.id)).toEqual(["existing", "s1"]);
    expect(w2.sessions).toEqual([]);
  });

  it("expands the workspace when the expanded set is not hydrated", () => {
    store.set(expandedWorkspaceIdsAtom, null);
    store.set(addAndActivateSessionAtom, makeSession("s1", "w1"));
    expect(store.get(expandedWorkspaceIdsAtom)).toEqual(new Set(["w1"]));
  });

  it("expands the workspace when the expanded set lacks it", () => {
    store.set(expandedWorkspaceIdsAtom, new Set(["w2"]));
    store.set(addAndActivateSessionAtom, makeSession("s1", "w1"));
    expect(store.get(expandedWorkspaceIdsAtom)).toEqual(new Set(["w2", "w1"]));
  });

  it("marks the session fresh and activates it", () => {
    store.set(addAndActivateSessionAtom, makeSession("s1", "w1"));
    expect(store.get(freshSessionsAtom).has("s1")).toBe(true);
    expect(store.get(activeSessionIdAtom)).toBe("s1");
  });
});
