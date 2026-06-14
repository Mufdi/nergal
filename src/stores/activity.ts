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

/// Complete the most recent running tool entry for a session (pairs a
/// PostToolUse with its PreToolUse): stamp duration + mark done. Prefers the
/// last running entry of the SAME tool; falls back to the last running tool of
/// any name. Returns whether a match was patched so the caller can fall back to
/// creating a standalone "done" entry.
export const completeToolActivityAtom = atom(
  null,
  (get, set, payload: { sessionId: string; toolName?: string }): boolean => {
    const list = get(activityMapAtom)[payload.sessionId];
    if (!list) return false;
    let idx = -1;
    for (let i = list.length - 1; i >= 0; i--) {
      const e = list[i];
      if (e.type !== "tool_use" || e.status !== "running") continue;
      if (payload.toolName && e.toolName === payload.toolName) {
        idx = i;
        break;
      }
      if (idx === -1) idx = i; // first running tool seen (fallback if no name match)
    }
    if (idx === -1) return false;
    const matched = list[idx];
    const patched = { ...matched, status: "done" as const, durationMs: Date.now() - matched.timestamp };
    const next = [...list];
    next[idx] = patched;
    set(activityMapAtom, (prev) => ({ ...prev, [payload.sessionId]: next }));
    return true;
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
