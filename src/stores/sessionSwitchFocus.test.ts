import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

import {
  shortcutRegistryAtom,
  focusZoneAtom,
  focusedWorkspaceIdAtom,
  triggerResumeSessionAtom,
} from "@/stores/shortcuts";
import {
  workspacesAtom,
  activeSessionIdAtom,
  freshSessionsAtom,
  type Session,
  type Workspace,
} from "@/stores/workspace";
import { appStore } from "@/stores/jotaiStore";

function makeSession(id: string): Session {
  return {
    id,
    name: id,
    workspace_id: "ws-1",
    worktree_path: null,
    worktree_branch: null,
    merge_target: null,
    status: "idle",
    created_at: 0,
    updated_at: 0,
  };
}

const workspace: Workspace = {
  id: "ws-1",
  name: "ws",
  repo_path: "/tmp/ws",
  sessions: [makeSession("s1"), makeSession("s2")],
  created_at: 0,
  is_git: true,
};

function pressSession2() {
  const entry = appStore.get(shortcutRegistryAtom).find((a) => a.id === "session-2");
  if (!entry) throw new Error("session-2 not in registry");
  entry.handler();
}

beforeEach(() => {
  appStore.set(workspacesAtom, [workspace]);
  appStore.set(activeSessionIdAtom, "s1");
  appStore.set(focusedWorkspaceIdAtom, null);
  appStore.set(freshSessionsAtom, new Set<string>());
  appStore.set(triggerResumeSessionAtom, null);
  appStore.set(focusZoneAtom, "sidebar");
});

describe("session-N switch hands the focus zone to the terminal", () => {
  it("direct switch: activates the fresh session and moves the zone to terminal", () => {
    appStore.set(freshSessionsAtom, new Set(["s2"]));
    pressSession2();
    expect(appStore.get(activeSessionIdAtom)).toBe("s2");
    expect(appStore.get(focusZoneAtom)).toBe("terminal");
  });

  it("resume path: triggers resume for a session without a terminal", () => {
    pressSession2();
    expect(appStore.get(triggerResumeSessionAtom)).toBe("s2");
  });

  it("already-active fresh session: still moves the zone to terminal", () => {
    appStore.set(activeSessionIdAtom, "s2");
    appStore.set(freshSessionsAtom, new Set(["s2"]));
    pressSession2();
    expect(appStore.get(activeSessionIdAtom)).toBe("s2");
    expect(appStore.get(focusZoneAtom)).toBe("terminal");
  });
});
