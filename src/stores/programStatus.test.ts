import { describe, it, expect, vi } from "vitest";
import { createStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";

const handlers = vi.hoisted(() => ({} as Record<string, (payload: unknown) => void>));

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(() => Promise.resolve(undefined)),
  listen: vi.fn((name: string, cb: (payload: unknown) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => {});
  }),
}));

import { applyProgramStatus, programStatusMapAtom, type ProgramStatusEntry } from "@/stores/programStatus";
import { setupHookListeners } from "@/stores/hooks";

const working: ProgramStatusEntry = { state: "working", kind: null, msg: null };

describe("applyProgramStatus", () => {
  it("sets an entry for a new session", () => {
    const next = applyProgramStatus({}, { session_id: "ps-set", state: "blocked", kind: "trust", msg: "Trust folder?" });
    expect(next).toEqual({ "ps-set": { state: "blocked", kind: "trust", msg: "Trust folder?" } });
  });

  it("replaces the entry when the state changes", () => {
    const prev = { "ps-replace": working };
    const next = applyProgramStatus(prev, { session_id: "ps-replace", state: "done", kind: null, msg: null });
    expect(next["ps-replace"]).toEqual({ state: "done", kind: null, msg: null });
  });

  it("replaces the entry when only the msg changes", () => {
    const prev = { "ps-msg": { state: "blocked" as const, kind: "q", msg: "a" } };
    const next = applyProgramStatus(prev, { session_id: "ps-msg", state: "blocked", kind: "q", msg: "b" });
    expect(next["ps-msg"].msg).toBe("b");
  });

  it("returns the same map reference for an equal entry", () => {
    const prev = { "ps-same": working };
    const next = applyProgramStatus(prev, { session_id: "ps-same", state: "working", kind: null, msg: null });
    expect(next).toBe(prev);
  });

  it("removes the key when state is null", () => {
    const prev = { "ps-remove": working };
    expect(applyProgramStatus(prev, { session_id: "ps-remove", state: null, kind: null, msg: null })).toEqual({});
  });

  it("leaves other sessions untouched", () => {
    const other: ProgramStatusEntry = { state: "error", kind: null, msg: "boom" };
    const prev = { "ps-a": working, "ps-b": other };
    const next = applyProgramStatus(prev, { session_id: "ps-a", state: "idle", kind: null, msg: null });
    expect(next["ps-b"]).toBe(other);
    expect(next["ps-a"].state).toBe("idle");
  });
});

describe("session:program-status listener", () => {
  it("writes the payload into programStatusMapAtom", async () => {
    const store = createStore();
    const unlisteners: UnlistenFn[] = await setupHookListeners(store);
    handlers["session:program-status"]({ session_id: "ps-listen", state: "working", kind: null, msg: null });
    expect(store.get(programStatusMapAtom)["ps-listen"]).toEqual(working);
    unlisteners.forEach((u) => u());
  });

  it("keeps the root status when the session_end hook arrives", async () => {
    const store = createStore();
    const unlisteners: UnlistenFn[] = await setupHookListeners(store);
    store.set(programStatusMapAtom, { "ps-end": working });
    handlers["hook:event"]({ event_type: "session_end", nergal_session_id: "ps-end" });
    expect(store.get(programStatusMapAtom)["ps-end"]).toEqual(working);
    unlisteners.forEach((u) => u());
  });
});
