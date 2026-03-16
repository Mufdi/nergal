import { atom } from "jotai";
import type { ActivityEntry } from "@/lib/types";
import { activeSessionIdAtom } from "./workspace";

const MAX_ENTRIES = 200;

export const activityMapAtom = atom<Record<string, ActivityEntry[]>>({});

export const activeActivityAtom = atom<ActivityEntry[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  return get(activityMapAtom)[id] ?? [];
});

// Write atom that appends to a specific session's log
export const addActivityAtom = atom(
  null,
  (_get, set, payload: { sessionId: string; entry: ActivityEntry }) => {
    set(activityMapAtom, (prev) => {
      const existing = prev[payload.sessionId] ?? [];
      const updated = [...existing, payload.entry].slice(-MAX_ENTRIES);
      return { ...prev, [payload.sessionId]: updated };
    });
  },
);
