import { atom } from "jotai";
import type { ActivityEntry } from "@/lib/types";
import {
  appendEntryPinned,
  applySubagentStart,
  applySubagentStop,
  closeRunningMainToolRows,
  closeUnclaimedSubagentToolRows,
  pickToolRowToComplete,
} from "@/lib/agentTelemetry";
import { activeSessionIdAtom } from "./workspace";
import { sessionScopedMapAtom } from "./sessionScope";

const MAX_ENTRIES = 200;

export const activityMapAtom = sessionScopedMapAtom<ActivityEntry[]>();

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
      const updated = appendEntryPinned(existing, payload.entry).slice(-MAX_ENTRIES);
      return { ...prev, [payload.sessionId]: updated };
    });
  },
);

/// Complete the most recent running tool entry for a session (pairs a
/// PostToolUse with its PreToolUse): stamp duration + mark done. Prefers the
/// last running entry of the SAME tool; falls back to the last running tool of
/// any name except Agent/Task (those complete via SubagentStop). Returns
/// whether a match was patched so the caller can fall back to creating a
/// standalone "done" entry.
export const completeToolActivityAtom = atom(
  null,
  (get, set, payload: { sessionId: string; toolName?: string }): boolean => {
    const list = get(activityMapAtom)[payload.sessionId];
    if (!list) return false;
    const idx = pickToolRowToComplete(list, payload.toolName);
    if (idx === -1) return false;
    const matched = list[idx];
    const patched = { ...matched, status: "done" as const, durationMs: Date.now() - matched.timestamp };
    const next = [...list];
    next[idx] = patched;
    set(activityMapAtom, (prev) => ({ ...prev, [payload.sessionId]: next }));
    return true;
  },
);

export const startSubagentActivityAtom = atom(
  null,
  (
    get,
    set,
    payload: { sessionId: string; agentId: string; agentType: string; id: string; timestamp: number },
  ) => {
    const list = get(activityMapAtom)[payload.sessionId] ?? [];
    const next = applySubagentStart(list, payload).slice(-MAX_ENTRIES);
    set(activityMapAtom, (prev) => ({ ...prev, [payload.sessionId]: next }));
  },
);

export const closeUnclaimedSubagentRowsAtom = atom(null, (get, set, sessionId: string) => {
  const list = get(activityMapAtom)[sessionId];
  if (!list) return;
  const next = closeUnclaimedSubagentToolRows(list, Date.now());
  if (next !== list) set(activityMapAtom, (prev) => ({ ...prev, [sessionId]: next }));
});

export const closeMainToolRowsAtom = atom(null, (get, set, sessionId: string) => {
  const list = get(activityMapAtom)[sessionId];
  if (!list) return;
  const next = closeRunningMainToolRows(list);
  if (next !== list) set(activityMapAtom, (prev) => ({ ...prev, [sessionId]: next }));
});

export const completeSubagentActivityAtom = atom(
  null,
  (
    get,
    set,
    payload: {
      sessionId: string;
      agentId: string;
      agentType: string;
      message?: string | null;
      effort?: string | null;
      tokens?: number | null;
      id: string;
      timestamp: number;
    },
  ) => {
    const list = get(activityMapAtom)[payload.sessionId] ?? [];
    const next = applySubagentStop(list, payload).slice(-MAX_ENTRIES);
    set(activityMapAtom, (prev) => ({ ...prev, [payload.sessionId]: next }));
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

/// Clear the active session's activity log.
export const clearActivityAtom = atom(null, (get, set) => {
  const id = get(activeSessionIdAtom);
  if (!id) return;
  set(activityMapAtom, (prev) => ({ ...prev, [id]: [] }));
});

/// Controls whether the activity drawer is open.
export const activityDrawerOpenAtom = atom(false);
