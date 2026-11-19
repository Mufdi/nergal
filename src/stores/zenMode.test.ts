import { describe, expect, it } from "vitest";
import { createStore } from "jotai";
import { closeZenModeAtom, openZenModeAtom, zenModeAtom } from "./zenMode";

const files = ["/repo/a.ts", "/repo/b.ts"];

describe("zen mode commit target", () => {
  it("opening with a commit stores its hash and scope", () => {
    const store = createStore();
    const commit = {
      hash: "abc1234",
      scope: { kind: "session" as const, sessionId: "s1" },
    };
    store.set(openZenModeAtom, { filePath: files[1], sessionId: "s1", files, commit });
    const state = store.get(zenModeAtom);
    expect(state.commit).toEqual(commit);
    expect(state.currentIndex).toBe(1);
  });

  it("opening without a commit leaves it null", () => {
    const store = createStore();
    store.set(openZenModeAtom, { filePath: files[0], sessionId: "s1", files });
    expect(store.get(zenModeAtom).commit).toBeNull();
  });

  it("closing resets the commit to null", () => {
    const store = createStore();
    store.set(openZenModeAtom, {
      filePath: files[0],
      sessionId: "s1",
      files,
      commit: { hash: "abc1234", scope: { kind: "workspace", workspaceId: "w1" } },
    });
    store.set(closeZenModeAtom);
    expect(store.get(zenModeAtom).commit).toBeNull();
  });
});
