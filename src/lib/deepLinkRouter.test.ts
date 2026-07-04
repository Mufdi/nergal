import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@/lib/tauri", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
  // Pulled in transitively via @/stores/workspace -> shortcuts -> quake,
  // which calls listen() at module scope to watch PTY exit events.
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "pty") => `${prefix}-test`,
}));

import { dispatchDeepLink } from "@/lib/deepLinkRouter";
import { getActiveConfirm, resolveConfirm } from "@/lib/confirm";
import { appStore } from "@/stores/jotaiStore";
import {
  workspacesAtom,
  activeSessionIdAtom,
  freshSessionsAtom,
  expandedWorkspaceIdsAtom,
  type Workspace,
  type Session,
} from "@/stores/workspace";

function makeWorkspace(repoPath: string, id = "ws1"): Workspace {
  return {
    id,
    name: repoPath.split("/").pop() ?? id,
    repo_path: repoPath,
    sessions: [],
    created_at: 0,
    is_git: true,
  };
}

function makeSession(workspaceId: string, id = "s1"): Session {
  return {
    id,
    name: "s",
    workspace_id: workspaceId,
    worktree_path: null,
    worktree_branch: null,
    merge_target: null,
    status: "idle",
    created_at: 0,
    updated_at: 0,
  };
}

/// Waits for the router's async chain to reach a pending confirm — the
/// gate must appear BEFORE any create_workspace/create_session/
/// queue_session_prompt call, so tests assert on this instead of guessing
/// a fixed number of microtask flushes.
async function waitForPendingConfirm() {
  await vi.waitFor(() => {
    if (!getActiveConfirm()) throw new Error("no pending confirm yet");
  });
  return getActiveConfirm()!;
}

async function waitForInvoke(cmd: string) {
  await vi.waitFor(() => {
    if (!invokeMock.mock.calls.some((c) => c[0] === cmd)) {
      throw new Error(`${cmd} not called yet`);
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "probe_workspace_path":
        return { is_dir: true, is_git_repo: true, resolved: args?.path };
      case "resolve_default_agent":
        return null;
      case "resolve_repo_root":
        return "/known/root";
      case "create_workspace":
        return makeWorkspace(args?.repoPath as string);
      case "create_session":
        return makeSession(args?.workspaceId as string);
      case "queue_session_prompt":
        return undefined;
      default:
        throw new Error(`unmocked invoke: ${cmd}`);
    }
  });
  appStore.set(workspacesAtom, []);
  appStore.set(activeSessionIdAtom, null);
  appStore.set(freshSessionsAtom, new Set());
  appStore.set(expandedWorkspaceIdsAtom, new Set());
  // Drain any confirm left pending by a previous test that threw before resolving.
  while (getActiveConfirm()) resolveConfirm(false);
});

describe("session/new", () => {
  it("shows a non-Enter-proceeding confirm before creating anything", async () => {
    dispatchDeepLink("nergal://session/new?cwd=/tmp/proj&prompt=hello");
    const pending = await waitForPendingConfirm();
    expect(pending.opts.enterConfirms).toBe(false);
    expect(pending.opts.body).toContain("/tmp/proj");
    expect(pending.opts.body).toContain("hello");
    resolveConfirm(false);
  });

  it("a declined confirm creates nothing", async () => {
    dispatchDeepLink("nergal://session/new?cwd=/tmp/proj&prompt=hello");
    await waitForPendingConfirm();
    resolveConfirm(false);
    // Give the router's post-decline branch (return) a chance to run.
    await Promise.resolve();
    await Promise.resolve();

    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls).not.toContain("create_workspace");
    expect(calls).not.toContain("create_session");
    expect(calls).not.toContain("queue_session_prompt");
  });

  it("a confirmed link creates the workspace, then the session, then queues the prompt, in order", async () => {
    dispatchDeepLink("nergal://session/new?cwd=/tmp/proj&prompt=hello");
    await waitForPendingConfirm();
    resolveConfirm(true);
    await waitForInvoke("queue_session_prompt");

    const order = invokeMock.mock.calls
      .map((c) => c[0])
      .filter((cmd) => ["create_workspace", "create_session", "queue_session_prompt"].includes(cmd));
    expect(order).toEqual(["create_workspace", "create_session", "queue_session_prompt"]);
  });
});

describe("open-file", () => {
  it("gates the unknown-path branch and creates nothing on decline", async () => {
    dispatchDeepLink("nergal://open-file?path=/tmp/proj/src/main.rs");
    const pending = await waitForPendingConfirm();
    expect(pending.opts.enterConfirms).toBe(false);
    resolveConfirm(false);
    await Promise.resolve();
    await Promise.resolve();

    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls).not.toContain("create_workspace");
    expect(calls).not.toContain("create_session");
  });

  it("a confirmed link creates the workspace and the session", async () => {
    dispatchDeepLink("nergal://open-file?path=/tmp/proj/src/main.rs");
    await waitForPendingConfirm();
    resolveConfirm(true);
    await waitForInvoke("create_session");

    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls).toContain("create_workspace");
    expect(calls).toContain("create_session");
  });

  it("does not gate opening a file already inside a known workspace", async () => {
    const ws = { ...makeWorkspace("/tmp/known"), sessions: [makeSession("ws1")] };
    appStore.set(workspacesAtom, [ws]);
    appStore.set(activeSessionIdAtom, ws.sessions[0].id);

    dispatchDeepLink("nergal://open-file?path=/tmp/known/src/main.rs");
    await Promise.resolve();
    await Promise.resolve();

    expect(getActiveConfirm()).toBeNull();
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("create_workspace");
  });

  it("gates session creation in a known workspace that has no session yet", async () => {
    // A known workspace with zero sessions: opening a file there would create
    // AND activate a session (spawning the agent). That spawn must be gated
    // even though no workspace is created.
    const ws = { ...makeWorkspace("/tmp/known"), sessions: [] };
    appStore.set(workspacesAtom, [ws]);
    appStore.set(activeSessionIdAtom, null);

    dispatchDeepLink("nergal://open-file?path=/tmp/known/src/main.rs");
    const pending = await waitForPendingConfirm();
    expect(pending.opts.enterConfirms).toBe(false);
    resolveConfirm(false);
    await Promise.resolve();
    await Promise.resolve();

    const calls = invokeMock.mock.calls.map((c) => c[0]);
    expect(calls).not.toContain("create_workspace");
    expect(calls).not.toContain("create_session");
  });
});

describe("open-workspace", () => {
  it("gates an unknown path and creates no workspace on decline", async () => {
    dispatchDeepLink("nergal://open-workspace?path=/tmp/other");
    const pending = await waitForPendingConfirm();
    expect(pending.opts.enterConfirms).toBe(false);
    resolveConfirm(false);
    await Promise.resolve();
    await Promise.resolve();

    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("create_workspace");
  });

  it("does not gate focusing an already-known workspace", async () => {
    const ws = { ...makeWorkspace("/tmp/known"), sessions: [makeSession("ws1")] };
    appStore.set(workspacesAtom, [ws]);

    dispatchDeepLink("nergal://open-workspace?path=/tmp/known");
    await Promise.resolve();
    await Promise.resolve();

    expect(getActiveConfirm()).toBeNull();
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("create_workspace");
  });
});

describe("pending confirm queue", () => {
  it("a second deep link arriving while one confirm is pending is queued, not coalesced", async () => {
    dispatchDeepLink("nergal://session/new?cwd=/tmp/one&prompt=first-link");
    const firstPending = await waitForPendingConfirm();
    expect(firstPending.opts.body).toContain("first-link");

    dispatchDeepLink("nergal://session/new?cwd=/tmp/two&prompt=second-link");
    await Promise.resolve();
    await Promise.resolve();
    // Still showing the first confirm — the second must not replace it.
    expect(getActiveConfirm()).toBe(firstPending);

    resolveConfirm(false);
    const secondPending = await waitForPendingConfirm();
    expect(secondPending).not.toBe(firstPending);
    expect(secondPending.opts.body).toContain("second-link");
    resolveConfirm(false);
  });
});
