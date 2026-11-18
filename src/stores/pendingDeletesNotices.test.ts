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
import { deleteSessionWithGraceAction, noticeToast } from "./pendingDeletes";
import {
  activeSessionIdAtom,
  sessionTabIdsAtom,
  workspacesAtom,
  type Session,
  type Workspace,
} from "./workspace";

const GRACE_AND_SLACK_MS = 7_000;
const RESTORE = "deleted; restore with git branch nergal/x abc1234";

const target: Session = {
  id: "pdn-a",
  name: "pdn-a",
  workspace_id: "w-pdn",
  worktree_path: "/repo/.worktrees/nergal/x",
  worktree_branch: "nergal/x",
  merge_target: "main",
  status: "idle",
  created_at: 100,
  updated_at: 100,
};

const workspace: Workspace = {
  id: "w-pdn",
  name: "w",
  repo_path: "/repo",
  sessions: [target],
  created_at: 1,
  is_git: true,
};

describe("noticeToast", () => {
  it.each([
    ["worktree_left", "Worktree left on disk", "error"],
    ["branch_kept", "Branch kept", "info"],
    ["branch_deleted", "Branch deleted", "info"],
  ] as const)("maps %s to its title and type", (kind, message, type) => {
    expect(noticeToast({ kind, message: "m" })).toEqual({ message, description: "m", type });
  });
});

describe("a successful session delete that returns notices", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockReset();
    Object.values(sileoMock).forEach((fn) => fn.mockClear());
    appStore.set(workspacesAtom, [workspace]);
    appStore.set(sessionTabIdsAtom, ["pdn-a"]);
    appStore.set(activeSessionIdAtom, "pdn-a");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows exactly one info toast titled Branch deleted with the restore command", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_session") return [{ kind: "branch_deleted", message: RESTORE }];
      throw new Error(`unexpected ${cmd}`);
    });

    appStore.set(deleteSessionWithGraceAction, target);
    await vi.advanceTimersByTimeAsync(GRACE_AND_SLACK_MS);

    expect(sileoMock.info).toHaveBeenCalledTimes(1);
    expect(sileoMock.info).toHaveBeenCalledWith(
      expect.objectContaining({ title: "Branch deleted", description: RESTORE }),
    );
    expect(sileoMock.error).not.toHaveBeenCalled();
  });
});
