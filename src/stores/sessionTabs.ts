import { atom } from "jotai";
import { createElement, useEffect, useState } from "react";
import { sileo } from "sileo";
import { activeSessionIdAtom, sessionTabIdsAtom, workspacesAtom } from "./workspace";
import { appStore } from "./jotaiStore";
import * as terminalService from "@/components/terminal/terminalService";

const SOFT_CLOSE_TTL_MS = 5_000;

/// Live countdown shown inside the soft-close toast. Keeps its own interval
/// so the parent toast doesn't need to re-render — the React element
/// captured by sileo as `description` mounts once and ticks itself. Cleans
/// up the interval on unmount (toast dismiss / undo / TTL expire). Built
/// with `createElement` because this file is .ts (not .tsx) — moving it to
/// .tsx would force a cascade of import-path updates for one tiny widget.
function CountdownLabel({ sessionName, deadline }: { sessionName: string; deadline: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 200);
    return () => clearInterval(id);
  }, []);
  const remainingMs = Math.max(0, deadline - now);
  const seconds = Math.ceil(remainingMs / 1000);
  return createElement(
    "span",
    null,
    `"${sessionName}" — undo within `,
    createElement(
      "span",
      { style: { fontVariantNumeric: "tabular-nums", fontWeight: 600 } },
      `${seconds}s`,
    ),
  );
}

/// Snapshot captured at soft-close time so the undo can restore the session
/// to its prior position in the tab bar and re-activate it without spawning
/// a new PTY. The terminalService entry stays alive (just `display:none`) for
/// the TTL window — undo is therefore instant.
export interface PendingSessionClose {
  sessionId: string;
  /// Original index in `sessionTabIdsAtom`. Used so undo restores at the
  /// same slot rather than appending at the end.
  tabIndex: number;
  /// Whether this session was the active one when closed; controls whether
  /// undo also flips `activeSessionIdAtom` back.
  wasActive: boolean;
  /// Active session at close-time. Restored on undo so the visible session
  /// returns to whatever the user was on before pressing Ctrl+W.
  prevActiveId: string | null;
  /// New active session chosen at close-time (the one focused after the
  /// close). Carried so undo can revert that switch as well.
  closeActiveId: string | null;
  closedAt: number;
  toastId: string;
}

export const pendingSessionClosesAtom = atom<PendingSessionClose[]>([]);

/// Timers live outside Jotai state — they're disposable resources, not data.
/// Keyed by sessionId; replaced if the same session is somehow soft-closed
/// twice (shouldn't happen, but defensive).
const finalizeTimers = new Map<string, ReturnType<typeof setTimeout>>();

function clearTimerFor(sessionId: string): void {
  const handle = finalizeTimers.get(sessionId);
  if (handle !== undefined) {
    clearTimeout(handle);
    finalizeTimers.delete(sessionId);
  }
}

function findSessionName(sessionId: string): string {
  const workspaces = appStore.get(workspacesAtom);
  for (const ws of workspaces) {
    for (const s of ws.sessions) {
      if (s.id === sessionId) return s.name;
    }
  }
  return "Session";
}

/// Soft-close a session tab: removes it from the tab bar and triggers a 5s
/// finalize timer. The PTY and terminal entry stay alive during the window
/// so `undoSessionCloseAction` can restore the session instantly. Shows an
/// action toast with an Undo button. No-op if the session isn't currently
/// in the tab bar.
export const softCloseSessionAction = atom(null, (get, set, sessionId: string) => {
  const tabIds = get(sessionTabIdsAtom);
  const tabIndex = tabIds.indexOf(sessionId);
  if (tabIndex === -1) return;

  const prevActiveId = get(activeSessionIdAtom);
  const wasActive = prevActiveId === sessionId;

  const remaining = tabIds.filter((id) => id !== sessionId);
  set(sessionTabIdsAtom, remaining);

  let closeActiveId: string | null = prevActiveId;
  if (wasActive) {
    /// Mirror TopBar's prior behaviour: focus the now-rightmost tab, or null
    /// if no tabs remain. The right panel's empty-state covers the null case.
    closeActiveId = remaining.length > 0 ? remaining[remaining.length - 1] : null;
    set(activeSessionIdAtom, closeActiveId);
  }

  const sessionName = findSessionName(sessionId);
  const deadline = Date.now() + SOFT_CLOSE_TTL_MS;
  const toastId = sileo.action({
    title: "Session closed",
    description: createElement(CountdownLabel, { sessionName, deadline }),
    duration: SOFT_CLOSE_TTL_MS,
    fill: "#171717",
    button: {
      title: "Undo",
      onClick: () => appStore.set(undoSessionCloseAction, sessionId),
    },
  });

  set(pendingSessionClosesAtom, (prev) => [
    ...prev,
    { sessionId, tabIndex, wasActive, prevActiveId, closeActiveId, closedAt: Date.now(), toastId },
  ]);

  clearTimerFor(sessionId);
  const handle = setTimeout(() => {
    appStore.set(finalizeSessionCloseAction, sessionId);
  }, SOFT_CLOSE_TTL_MS);
  finalizeTimers.set(sessionId, handle);
});

/// Restore a soft-closed session: re-inserts it into the tab bar at its
/// original index, re-activates it if it was active at close-time, cancels
/// the finalize timer, and dismisses the undo toast. Without an arg, pops
/// the most recent pending close (LIFO) — this is what Ctrl+Shift+T uses.
export const undoSessionCloseAction = atom(null, (get, set, sessionId?: string) => {
  const pending = get(pendingSessionClosesAtom);
  if (pending.length === 0) return;

  const target = sessionId
    ? pending.find((p) => p.sessionId === sessionId)
    : pending[pending.length - 1];
  if (!target) return;

  clearTimerFor(target.sessionId);
  sileo.dismiss(target.toastId);

  set(sessionTabIdsAtom, (prev) => {
    if (prev.includes(target.sessionId)) return prev;
    const next = [...prev];
    const insertAt = Math.min(target.tabIndex, next.length);
    next.splice(insertAt, 0, target.sessionId);
    return next;
  });

  if (target.wasActive) {
    set(activeSessionIdAtom, target.sessionId);
  }

  set(pendingSessionClosesAtom, (prev) => prev.filter((p) => p.sessionId !== target.sessionId));
});

/// Permanently destroy a soft-closed session: kills the PTY, removes the
/// terminal container, drops the pending entry. Called by the finalize
/// timer; idempotent.
export const finalizeSessionCloseAction = atom(null, (get, set, sessionId: string) => {
  clearTimerFor(sessionId);
  const pending = get(pendingSessionClosesAtom);
  const entry = pending.find((p) => p.sessionId === sessionId);
  if (!entry) return;

  terminalService.destroy(sessionId);
  set(pendingSessionClosesAtom, pending.filter((p) => p.sessionId !== sessionId));
});

/// True if there's at least one session in the soft-close window. Used by
/// the Ctrl+Shift+T handler to decide whether to undo a session-close vs
/// fall through to the panel-tab reopen stack.
export const hasPendingSessionCloseAtom = atom((get) => get(pendingSessionClosesAtom).length > 0);
