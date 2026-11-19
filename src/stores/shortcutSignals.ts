import type { PrimitiveAtom } from "jotai";
import { triggerJumpToProjectAtom } from "@/stores/shortcuts";

interface SignalStore {
  get: <T>(a: PrimitiveAtom<T>) => T;
  set: <T>(a: PrimitiveAtom<T>, v: T) => void;
}

// The consumers live in WorkspacesView, which unmounts when the sidebar
// collapses; clearing the signal on use keeps a remount from replaying it.
export function consumeCounterSignal(store: SignalStore, signal: PrimitiveAtom<number>, canAct = true): boolean {
  if (store.get(signal) <= 0 || !canAct) return false;
  store.set(signal, 0);
  return true;
}

export function consumeJumpSignal(store: SignalStore, workspaceCount = Infinity): number | null {
  const { tick, index } = store.get(triggerJumpToProjectAtom);
  if (tick === 0 || workspaceCount === 0) return null;
  store.set(triggerJumpToProjectAtom, { tick: 0, index: -1 });
  return index < workspaceCount ? index : null;
}
