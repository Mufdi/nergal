import { describe, it, expect } from "vitest";
import { appStore } from "@/stores/jotaiStore";
import {
  conflictKey,
  conflictStateMapAtom,
  conflictIntentMapAtom,
  pruneConflictSessionAction,
  type ConflictState,
} from "@/stores/conflict";

const state = (loaded: boolean): ConflictState => ({
  ours: "a",
  theirs: "b",
  merged: "c",
  originalMerged: "c",
  loaded,
});

describe("pruneConflictSessionAction", () => {
  it("removes only the pruned session's composite-keyed entries", () => {
    appStore.set(conflictStateMapAtom, {
      [conflictKey("session-a", "file.ts")]: state(true),
      [conflictKey("session-b", "file.ts")]: state(false),
    });
    appStore.set(conflictIntentMapAtom, {
      [conflictKey("session-a", "file.ts")]: "resolve please",
      [conflictKey("session-b", "file.ts")]: "keep theirs",
    });

    appStore.set(pruneConflictSessionAction, "session-a");

    expect(appStore.get(conflictStateMapAtom)).toEqual({
      [conflictKey("session-b", "file.ts")]: state(false),
    });
    expect(appStore.get(conflictIntentMapAtom)).toEqual({
      [conflictKey("session-b", "file.ts")]: "keep theirs",
    });
  });

  it("does not prune a session whose id is a string-prefix of another session's id", () => {
    appStore.set(conflictStateMapAtom, {
      [conflictKey("session-1", "file.ts")]: state(true),
      [conflictKey("session-10", "file.ts")]: state(false),
    });

    appStore.set(pruneConflictSessionAction, "session-1");

    expect(appStore.get(conflictStateMapAtom)).toEqual({
      [conflictKey("session-10", "file.ts")]: state(false),
    });
  });

  it("removes multiple paths for the same pruned session", () => {
    appStore.set(conflictStateMapAtom, {
      [conflictKey("session-a", "a.ts")]: state(true),
      [conflictKey("session-a", "b.ts")]: state(true),
      [conflictKey("session-b", "a.ts")]: state(false),
    });

    appStore.set(pruneConflictSessionAction, "session-a");

    expect(appStore.get(conflictStateMapAtom)).toEqual({
      [conflictKey("session-b", "a.ts")]: state(false),
    });
  });
});
