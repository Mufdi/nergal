import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

import {
  shortcutRegistryAtom,
  triggerAddWorkspaceAtom,
  triggerNewSessionAtom,
  triggerJumpToProjectAtom,
} from "@/stores/shortcuts";
import { consumeCounterSignal, consumeJumpSignal } from "@/stores/shortcutSignals";
import { appStore } from "@/stores/jotaiStore";

function press(id: string) {
  const entry = appStore.get(shortcutRegistryAtom).find((a) => a.id === id);
  if (!entry) throw new Error(`shortcut ${id} not in registry`);
  entry.handler();
}

beforeEach(() => {
  appStore.set(triggerAddWorkspaceAtom, 0);
  appStore.set(triggerNewSessionAtom, 0);
  appStore.set(triggerJumpToProjectAtom, { tick: 0, index: -1 });
});

describe.each([
  ["add-workspace", triggerAddWorkspaceAtom],
  ["new-session", triggerNewSessionAtom],
])("consumeCounterSignal via %s", (id, signal) => {
  it("is false at rest", () => {
    expect(consumeCounterSignal(appStore, signal)).toBe(false);
  });

  it("is true once after a press and leaves the atom at 0", () => {
    press(id);
    expect(consumeCounterSignal(appStore, signal)).toBe(true);
    expect(appStore.get(signal)).toBe(0);
  });

  it("is false on a second call, so a remount does not replay the effect", () => {
    press(id);
    consumeCounterSignal(appStore, signal);
    expect(consumeCounterSignal(appStore, signal)).toBe(false);
  });

  it("is true again after another press", () => {
    press(id);
    consumeCounterSignal(appStore, signal);
    press(id);
    expect(consumeCounterSignal(appStore, signal)).toBe(true);
  });
});

describe("consumeJumpSignal", () => {
  it("is null at rest", () => {
    expect(consumeJumpSignal(appStore)).toBeNull();
  });

  it("returns the index once after project-1 and resets the atom", () => {
    press("project-1");
    expect(consumeJumpSignal(appStore)).toBe(0);
    expect(appStore.get(triggerJumpToProjectAtom)).toEqual({ tick: 0, index: -1 });
  });

  it("is null on a second call", () => {
    press("project-1");
    consumeJumpSignal(appStore);
    expect(consumeJumpSignal(appStore)).toBeNull();
  });
});

describe("consumeCounterSignal with preconditions", () => {
  it("keeps a pending signal while canAct is false and consumes it once true", () => {
    press("add-workspace");
    expect(consumeCounterSignal(appStore, triggerAddWorkspaceAtom, false)).toBe(false);
    expect(appStore.get(triggerAddWorkspaceAtom)).toBe(1);
    expect(consumeCounterSignal(appStore, triggerAddWorkspaceAtom, true)).toBe(true);
    expect(appStore.get(triggerAddWorkspaceAtom)).toBe(0);
  });
});

describe("consumeJumpSignal with a workspace count", () => {
  it("keeps the signal while no workspaces are loaded, then yields the index", () => {
    press("project-1");
    expect(consumeJumpSignal(appStore, 0)).toBeNull();
    expect(appStore.get(triggerJumpToProjectAtom).tick).toBeGreaterThan(0);
    expect(consumeJumpSignal(appStore, 3)).toBe(0);
  });

  it("drops an index beyond the workspace count and resets the signal", () => {
    press("project-5");
    expect(consumeJumpSignal(appStore, 3)).toBeNull();
    expect(appStore.get(triggerJumpToProjectAtom)).toEqual({ tick: 0, index: -1 });
  });
});
