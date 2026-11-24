import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createStore } from "jotai";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@/lib/tauri", () => ({
  invoke,
  listen: () => Promise.resolve(() => {}),
  generateId: (prefix = "id") => `${prefix}-test`,
}));

import {
  prInFlightMapAtom,
  prInfoMapAtom,
  prSignalMapAtom,
  refreshPrInfoAction,
  setPrInfoFromShip,
} from "@/stores/git";
import { workspacesAtom, type Session, type Workspace } from "@/stores/workspace";

const PR_OPEN = { number: 12, title: "t", state: "OPEN", url: "u12" };
const PR_MERGED = { number: 9, title: "old", state: "MERGED", url: "u9" };

function session(id: string): Session {
  return {
    id,
    name: id,
    workspace_id: "w1",
    worktree_path: "/wt",
    worktree_branch: "feat",
    merge_target: "main",
    status: "idle",
    created_at: 0,
    updated_at: 0,
  };
}

function storeWithSession(signal?: string) {
  const store = createStore();
  const ws: Workspace = {
    id: "w1",
    name: "w",
    repo_path: "/r",
    sessions: [session("s1")],
    created_at: 0,
    is_git: true,
  };
  store.set(workspacesAtom, [ws]);
  if (signal !== undefined) store.set(prSignalMapAtom, { s1: signal });
  return store;
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("refreshPrInfoAction", () => {
  beforeEach(() => {
    invoke.mockReset();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(5_000_000);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("writes data, fetchedAt and the session's current signal", async () => {
    const store = storeWithSession("pr:12");
    invoke.mockResolvedValue(PR_OPEN);
    const before = Date.now();
    await store.set(refreshPrInfoAction, "s1");
    const entry = store.get(prInfoMapAtom).s1;
    expect(invoke).toHaveBeenCalledWith("get_pr_status", { sessionId: "s1" });
    expect(entry.data).toEqual(PR_OPEN);
    expect(entry.signal).toBe("pr:12");
    expect(entry.fetchedAt).toBeGreaterThanOrEqual(before);
  });

  it("keeps previous data and stamps signal and fetchedAt on a rejected invoke", async () => {
    const store = storeWithSession("pr:12");
    store.set(prInfoMapAtom, { s1: { data: PR_OPEN, fetchedAt: 111, signal: "none" } });
    invoke.mockRejectedValue(new Error("gh failed"));
    await store.set(refreshPrInfoAction, "s1");
    const entry = store.get(prInfoMapAtom).s1;
    expect(entry.data).toEqual(PR_OPEN);
    expect(entry.signal).toBe("pr:12");
    expect(entry.fetchedAt).toBe(5_000_000);
  });

  it("keeps previous data and stamps signal and fetchedAt on a null result under pr:12", async () => {
    const store = storeWithSession("pr:12");
    store.set(prInfoMapAtom, { s1: { data: PR_OPEN, fetchedAt: 111, signal: "none" } });
    invoke.mockResolvedValue(null);
    await store.set(refreshPrInfoAction, "s1");
    const entry = store.get(prInfoMapAtom).s1;
    expect(entry.data).toEqual(PR_OPEN);
    expect(entry.signal).toBe("pr:12");
    expect(entry.fetchedAt).toBe(5_000_000);
  });

  it("caches a resolved PR with a different number under pr:12", async () => {
    const store = storeWithSession("pr:12");
    store.set(prInfoMapAtom, { s1: { data: PR_OPEN, fetchedAt: 111, signal: "pr:12" } });
    const other = { ...PR_OPEN, number: 30, url: "u30" };
    invoke.mockResolvedValue(other);
    await store.set(refreshPrInfoAction, "s1");
    const entry = store.get(prInfoMapAtom).s1;
    expect(entry.data?.number).toBe(30);
    expect(entry.signal).toBe("pr:12");
    expect(entry.fetchedAt).toBe(5_000_000);
  });

  it("does not call invoke again while a refresh is pending", async () => {
    const store = storeWithSession("pr:12");
    const first = deferred<typeof PR_OPEN | null>();
    invoke.mockReturnValueOnce(first.promise);
    const p1 = store.set(refreshPrInfoAction, "s1");
    const p2 = store.set(refreshPrInfoAction, "s1");
    first.resolve(PR_OPEN);
    await Promise.all([p1, p2]);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("records a MERGED PR when the signal is none", async () => {
    const store = storeWithSession("none");
    invoke.mockResolvedValue(PR_MERGED);
    await store.set(refreshPrInfoAction, "s1");
    const entry = store.get(prInfoMapAtom).s1;
    expect(entry.data).toEqual(PR_MERGED);
    expect(entry.signal).toBe("none");
  });

  it("writes nothing for a session missing from the workspaces", async () => {
    const store = createStore();
    store.set(workspacesAtom, []);
    invoke.mockResolvedValue(PR_OPEN);
    await store.set(refreshPrInfoAction, "gone");
    expect(store.get(prInfoMapAtom)).toEqual({});
  });

  it("setPrInfoFromShip writes the entry without invoking", () => {
    const store = storeWithSession("pr:12");
    store.set(setPrInfoFromShip, { sessionId: "s1", prInfo: PR_OPEN });
    expect(invoke).not.toHaveBeenCalled();
    expect(store.get(prInfoMapAtom).s1).toEqual({
      data: PR_OPEN,
      fetchedAt: 5_000_000,
      signal: "pr:12",
    });
  });

  it("an older in-flight refresh does not overwrite setPrInfoFromShip", async () => {
    const store = storeWithSession("pr:12");
    const pending = deferred<typeof PR_OPEN | null>();
    invoke.mockReturnValueOnce(pending.promise);
    const refresh = store.set(refreshPrInfoAction, "s1");
    const shipped = { ...PR_OPEN, number: 12, title: "shipped" };
    store.set(setPrInfoFromShip, { sessionId: "s1", prInfo: shipped });
    pending.resolve(PR_OPEN);
    await refresh;
    expect(store.get(prInfoMapAtom).s1.data).toEqual(shipped);
  });

  it("a forced refresh during a pending one invokes again and its result wins over the older response", async () => {
    const store = storeWithSession("pr:12");
    const older = deferred<typeof PR_OPEN | null>();
    const forced = deferred<typeof PR_OPEN | null>();
    invoke.mockReturnValueOnce(older.promise).mockReturnValueOnce(forced.promise);
    const p1 = store.set(refreshPrInfoAction, "s1");
    const p2 = store.set(refreshPrInfoAction, { sessionId: "s1", force: true });
    expect(invoke).toHaveBeenCalledTimes(2);
    const merged = { ...PR_OPEN, state: "MERGED" };
    forced.resolve(merged);
    await p2;
    older.resolve(PR_OPEN);
    await p1;
    expect(store.get(prInfoMapAtom).s1.data).toEqual(merged);
    expect(store.get(prInFlightMapAtom)["s1"]).toBeUndefined();
  });

  it("the older call's settle does not clear the forced call's in-flight state", async () => {
    const store = storeWithSession("pr:12");
    const older = deferred<typeof PR_OPEN | null>();
    const forced = deferred<typeof PR_OPEN | null>();
    invoke.mockReturnValueOnce(older.promise).mockReturnValueOnce(forced.promise);
    const p1 = store.set(refreshPrInfoAction, "s1");
    const p2 = store.set(refreshPrInfoAction, { sessionId: "s1", force: true });
    older.resolve(PR_OPEN);
    await p1;
    expect(store.get(prInFlightMapAtom)["s1"]).toBeTruthy();
    forced.resolve(PR_OPEN);
    await p2;
    expect(store.get(prInFlightMapAtom)["s1"]).toBeUndefined();
  });

  it("a bare session id still refreshes", async () => {
    const store = storeWithSession("pr:12");
    invoke.mockResolvedValue(PR_OPEN);
    await store.set(refreshPrInfoAction, "s1");
    expect(store.get(prInfoMapAtom).s1.data).toEqual(PR_OPEN);
    expect(store.get(prInFlightMapAtom)["s1"]).toBeUndefined();
  });

  it("setPrInfoFromShip with number 0 refreshes via invoke instead of caching #0", async () => {
    const store = storeWithSession("pr:12");
    invoke.mockResolvedValue(PR_OPEN);
    store.set(setPrInfoFromShip, { sessionId: "s1", prInfo: { ...PR_OPEN, number: 0 } });
    await vi.waitFor(() => expect(store.get(prInfoMapAtom).s1?.data?.number).toBe(12));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_pr_status", { sessionId: "s1" });
  });

  it("leaves no in-flight key for a session removed while its fetch was pending", async () => {
    const store = storeWithSession("pr:12");
    const pending = deferred<typeof PR_OPEN | null>();
    invoke.mockReturnValueOnce(pending.promise);
    const refresh = store.set(refreshPrInfoAction, "s1");
    store.set(workspacesAtom, []);
    pending.resolve(PR_OPEN);
    await refresh;
    expect("s1" in store.get(prInFlightMapAtom)).toBe(false);
  });
});
