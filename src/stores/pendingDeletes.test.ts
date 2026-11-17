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
import { deleteSessionWithGraceAction } from "./pendingDeletes";
import {
  activeSessionIdAtom,
  sessionTabIdsAtom,
  workspacesAtom,
  type Session,
  type Workspace,
} from "./workspace";

const GRACE_AND_SLACK_MS = 7_000;
const REFUSAL = "The last repo-root session of a workspace cannot be deleted";

function session(id: string): Session {
  return {
    id,
    name: id,
    workspace_id: "w-pd",
    worktree_path: null,
    worktree_branch: null,
    merge_target: null,
    status: "idle",
    created_at: 100,
    updated_at: 100,
  };
}

function workspace(sessions: Session[]): Workspace {
  return { id: "w-pd", name: "w", repo_path: "/repo", sessions, created_at: 1, is_git: true };
}

const a = session("pd-a");
const b = session("pd-b");
const c = session("pd-c");

async function deleteMiddleSessionAndRunPastGrace() {
  appStore.set(deleteSessionWithGraceAction, b);
  await vi.advanceTimersByTimeAsync(GRACE_AND_SLACK_MS);
}

describe("a delete the backend refuses", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    Object.values(sileoMock).forEach((fn) => fn.mockClear());
    appStore.set(workspacesAtom, [workspace([a, b, c])]);
    appStore.set(sessionTabIdsAtom, ["pd-a", "pd-b", "pd-c"]);
    appStore.set(activeSessionIdAtom, "pd-b");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("restores the row, the tab and the active session and shows one error toast", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_session") throw new Error(REFUSAL);
      if (cmd === "get_workspaces") return [workspace([a, b, c])];
      throw new Error(`unexpected ${cmd}`);
    });

    await deleteMiddleSessionAndRunPastGrace();

    const ids = appStore.get(workspacesAtom)[0].sessions.map((s) => s.id);
    expect(ids).toEqual(["pd-a", "pd-b", "pd-c"]);
    expect(appStore.get(sessionTabIdsAtom)).toEqual(["pd-a", "pd-b", "pd-c"]);
    expect(appStore.get(activeSessionIdAtom)).toBe("pd-b");
    expect(sileoMock.error).toHaveBeenCalledTimes(1);
    expect(sileoMock.error).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Could not delete session", description: REFUSAL }),
    );
  });

  it("reinserts the row at its original index when reloading workspaces also fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_session") throw new Error(REFUSAL);
      throw new Error("backend unavailable");
    });

    await deleteMiddleSessionAndRunPastGrace();

    const ids = appStore.get(workspacesAtom)[0].sessions.map((s) => s.id);
    expect(ids).toEqual(["pd-a", "pd-b", "pd-c"]);
    expect(appStore.get(sessionTabIdsAtom)).toEqual(["pd-a", "pd-b", "pd-c"]);
    expect(appStore.get(activeSessionIdAtom)).toBe("pd-b");
    expect(sileoMock.error).toHaveBeenCalledTimes(1);
  });
});
