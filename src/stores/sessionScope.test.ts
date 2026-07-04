import { describe, it, expect, vi } from "vitest";
import { appStore } from "@/stores/jotaiStore";
import { sessionScopedMapAtom, pruneSessionStateAction } from "@/stores/sessionScope";

describe("sessionScopedMapAtom + pruneSessionStateAction", () => {
  it("removes only the pruned session's key, leaves other sessions intact", () => {
    const mapAtom = sessionScopedMapAtom<string>();
    appStore.set(mapAtom, { "session-a": "alpha", "session-b": "beta" });

    appStore.set(pruneSessionStateAction, "session-a");

    expect(appStore.get(mapAtom)).toEqual({ "session-b": "beta" });
  });

  it("is a no-op when the session has no entry in the map", () => {
    const mapAtom = sessionScopedMapAtom<string>();
    appStore.set(mapAtom, { "session-b": "beta" });

    appStore.set(pruneSessionStateAction, "session-a");

    expect(appStore.get(mapAtom)).toEqual({ "session-b": "beta" });
  });

  it("prunes the same session key across every registered map in one call", () => {
    const mapA = sessionScopedMapAtom<number>();
    const mapB = sessionScopedMapAtom<boolean>();
    appStore.set(mapA, { s1: 1, s2: 2 });
    appStore.set(mapB, { s1: true, s2: false });

    appStore.set(pruneSessionStateAction, "s1");

    expect(appStore.get(mapA)).toEqual({ s2: 2 });
    expect(appStore.get(mapB)).toEqual({ s2: false });
  });

  it("calls onPrune with the dropped value before removing it", () => {
    const onPrune = vi.fn();
    const mapAtom = sessionScopedMapAtom<{ label: string }>({}, onPrune);
    const dropped = { label: "disposable" };
    appStore.set(mapAtom, { "session-a": dropped });

    appStore.set(pruneSessionStateAction, "session-a");

    expect(onPrune).toHaveBeenCalledWith(dropped);
    expect(appStore.get(mapAtom)).toEqual({});
  });
});
