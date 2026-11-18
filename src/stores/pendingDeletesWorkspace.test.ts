import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const sileoMock = vi.hoisted(() => ({
  action: vi.fn(() => "toast-id"),
  dismiss: vi.fn(),
  error: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
}));
vi.mock("@/lib/tauri", () => ({ invoke: invokeMock }));
vi.mock("sileo", () => ({ sileo: sileoMock }));
vi.mock("@/components/terminal/terminalService", () => ({ destroy: vi.fn() }));

import { appStore } from "./jotaiStore";
import { deleteWorkspaceWithGraceAction } from "./pendingDeletes";
import {
  activeSessionIdAtom,
  sessionTabIdsAtom,
  workspacesAtom,
  type Session,
  type Workspace,
} from "./workspace";

const GRACE_AND_SLACK_MS = 7_000;
const REFUSAL = "could not remove worktree dir /repo/.worktrees/nergal/s1: docker failed";

function session(id: string, workspaceId: string): Session {
  return {
    id,
    name: id,
    workspace_id: workspaceId,
    worktree_path: null,
    worktree_branch: null,
    merge_target: null,
    status: "idle",
    created_at: 100,
    updated_at: 100,
  };
}

function workspace(id: string, sessions: Session[]): Workspace {
  return { id, name: id, repo_path: `/${id}`, sessions, created_at: 1, is_git: true };
}

const s1 = session("pw-s1", "pw-target");
const s2 = session("pw-s2", "pw-target");
const other = session("pw-other", "pw-neighbor");
const first = workspace("pw-first", []);
const target = workspace("pw-target", [s1, s2]);
const neighbor = workspace("pw-neighbor", [other]);

async function deleteTargetAndRunPastGrace() {
  appStore.set(deleteWorkspaceWithGraceAction, target);
  await vi.advanceTimersByTimeAsync(GRACE_AND_SLACK_MS);
}

describe("a delete_workspace the backend refuses", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    Object.values(sileoMock).forEach((fn) => fn.mockClear());
    appStore.set(workspacesAtom, [first, target, neighbor]);
    appStore.set(sessionTabIdsAtom, ["pw-other", "pw-s1", "pw-s2"]);
    appStore.set(activeSessionIdAtom, "pw-s1");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("puts the workspace back, restores its tab ids and shows one error toast", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_workspace") throw new Error(REFUSAL);
      if (cmd === "get_workspaces") return [first, target, neighbor];
      throw new Error(`unexpected ${cmd}`);
    });

    await deleteTargetAndRunPastGrace();

    const workspaces = appStore.get(workspacesAtom);
    expect(workspaces.map((w) => w.id)).toEqual(["pw-first", "pw-target", "pw-neighbor"]);
    expect(workspaces[1].sessions.map((s) => s.id)).toEqual(["pw-s1", "pw-s2"]);
    expect(appStore.get(sessionTabIdsAtom)).toEqual(["pw-other", "pw-s1", "pw-s2"]);
    expect(sileoMock.error).toHaveBeenCalledTimes(1);
    expect(sileoMock.error).toHaveBeenCalledWith(
      expect.objectContaining({ description: REFUSAL }),
    );
  });

  it("restores only the sessions the backend kept", async () => {
    const kept = workspace("pw-target", [s2]);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_workspace") throw new Error(REFUSAL);
      if (cmd === "get_workspaces") return [first, kept, neighbor];
      throw new Error(`unexpected ${cmd}`);
    });

    await deleteTargetAndRunPastGrace();

    const workspaces = appStore.get(workspacesAtom);
    expect(workspaces[1].sessions.map((s) => s.id)).toEqual(["pw-s2"]);
    expect(appStore.get(sessionTabIdsAtom)).toEqual(["pw-other", "pw-s2"]);
    expect(sileoMock.error).toHaveBeenCalledTimes(1);
  });

  it("reinserts the workspace at its original index when reloading also fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_workspace") throw new Error(REFUSAL);
      throw new Error("backend unavailable");
    });

    await deleteTargetAndRunPastGrace();

    const workspaces = appStore.get(workspacesAtom);
    expect(workspaces.map((w) => w.id)).toEqual(["pw-first", "pw-target", "pw-neighbor"]);
    expect(appStore.get(sessionTabIdsAtom)).toEqual(["pw-other", "pw-s1", "pw-s2"]);
    expect(sileoMock.error).toHaveBeenCalledTimes(1);
    expect(sileoMock.error).toHaveBeenCalledWith(
      expect.objectContaining({ description: REFUSAL }),
    );
  });

  it("leaves the workspace gone when the delete succeeds", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_workspace") return [];
      throw new Error(`unexpected ${cmd}`);
    });

    await deleteTargetAndRunPastGrace();

    expect(appStore.get(workspacesAtom).map((w) => w.id)).toEqual(["pw-first", "pw-neighbor"]);
    expect(sileoMock.error).not.toHaveBeenCalled();
  });
});
