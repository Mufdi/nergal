import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

import { shortcutRegistryAtom } from "@/stores/shortcuts";
import { consumeCounterSignal } from "@/stores/shortcutSignals";
import {
  focusWorktreeGateAtom,
  worktreeRequestsAtom,
  type WorktreeRequestView,
} from "@/stores/worktreeGate";
import { appStore } from "@/stores/jotaiStore";

function press(id: string) {
  const entry = appStore.get(shortcutRegistryAtom).find((a) => a.id === id);
  if (!entry) throw new Error(`shortcut ${id} not in registry`);
  entry.handler();
}

function makeRequest(): WorktreeRequestView {
  return {
    id: "req-1",
    kind: { type: "create" },
    requesting_session: "session-1",
    workspace_id: "workspace-1",
    repo_path: "/tmp/repo",
    branch_name: null,
    prompt: "do the thing",
    agent: null,
    permission_preset: null,
    startup_command: null,
    allow_skip_in_cycle: false,
    created_at: 0,
    timeout_secs: 60,
    resources: {
      worktree_count: 0,
      free_disk_bytes: 0,
      soft_cap: 10,
      over_soft_cap: false,
    },
  };
}

beforeEach(() => {
  appStore.set(focusWorktreeGateAtom, 0);
  appStore.set(worktreeRequestsAtom, []);
});

describe("focus-worktree-gate shortcut", () => {
  it("is registered as leader g in the session group", () => {
    const entry = appStore.get(shortcutRegistryAtom).find((a) => a.id === "focus-worktree-gate");
    expect(entry).toBeDefined();
    expect(entry?.keys).toBe("leader g");
    expect(entry?.group).toBe("session");
  });

  it("signals once when a request is pending", () => {
    appStore.set(worktreeRequestsAtom, [makeRequest()]);
    press("focus-worktree-gate");
    expect(consumeCounterSignal(appStore, focusWorktreeGateAtom)).toBe(true);
    expect(consumeCounterSignal(appStore, focusWorktreeGateAtom)).toBe(false);
  });

  it("leaves no stale signal when the queue is empty", () => {
    press("focus-worktree-gate");
    expect(appStore.get(focusWorktreeGateAtom)).toBe(0);
    expect(consumeCounterSignal(appStore, focusWorktreeGateAtom)).toBe(false);
  });
});
