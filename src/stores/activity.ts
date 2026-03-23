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

/// Summary for status bar: last action, total count, and session start time.
export const activitySummaryAtom = atom((get) => {
  const entries = get(activeActivityAtom);
  const toolEntries = entries.filter((e) => e.type === "tool_use");
  const last = toolEntries[toolEntries.length - 1] ?? null;
  const firstEntry = entries[0] ?? null;
  const elapsed = firstEntry ? Math.floor((Date.now() - firstEntry.timestamp) / 1000) : 0;
  return {
    lastAction: last?.message ?? null,
    actionCount: toolEntries.length,
    elapsedSeconds: elapsed,
  };
});

/// Controls whether the activity drawer is open.
export const activityDrawerOpenAtom = atom(false);
