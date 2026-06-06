import { atom } from "jotai";
import { createElement } from "react";
import { sileo } from "sileo";
import {
  workspacesAtom,
  activeSessionIdAtom,
  sessionTabIdsAtom,
  type Session,
  type Workspace,
} from "./workspace";
import { appStore } from "./jotaiStore";
import { CountdownLabel } from "./sessionTabs";
import * as terminalService from "@/components/terminal/terminalService";
import { invoke } from "@/lib/tauri";

const DELETE_GRACE_MS = 5_000;

/// Timers are disposable resources, not data — kept outside Jotai.
const graceTimers = new Map<string, ReturnType<typeof setTimeout>>();

function cancelTimer(key: string): void {
  const handle = graceTimers.get(key);
  if (handle !== undefined) {
    clearTimeout(handle);
    graceTimers.delete(key);
  }
}

function insertAt<T>(list: T[], item: T, index: number): T[] {
  const next = [...list];
  next.splice(Math.min(index, next.length), 0, item);
  return next;
}

/// Removes the session from the UI immediately but defers the destructive
/// part (PTY kill + DB delete + worktree removal) for a grace window with
/// an Undo toast. The PTY keeps running during the window, so undo is
/// instant and lossless.
export const deleteSessionWithGraceAction = atom(null, (get, set, session: Session) => {
  const workspaces = get(workspacesAtom);
  const ws = workspaces.find((w) => w.sessions.some((s) => s.id === session.id));
  if (!ws) return;
  const sessionIndex = ws.sessions.findIndex((s) => s.id === session.id);
  const tabIndex = get(sessionTabIdsAtom).indexOf(session.id);
  const wasActive = get(activeSessionIdAtom) === session.id;
  const wsId = ws.id;

  set(workspacesAtom, (prev) =>
    prev.map((w) => ({ ...w, sessions: w.sessions.filter((s) => s.id !== session.id) })),
  );
  set(sessionTabIdsAtom, (prev) => prev.filter((id) => id !== session.id));
  if (wasActive) set(activeSessionIdAtom, null);

  const deadline = Date.now() + DELETE_GRACE_MS;
  const toastId = sileo.action({
    title: "Session deleted",
    description: createElement(CountdownLabel, { sessionName: session.name, deadline }),
    duration: DELETE_GRACE_MS,
    fill: "#171717",
    button: {
      title: "Undo",
      onClick: () => {
        cancelTimer(session.id);
        sileo.dismiss(toastId);
        appStore.set(workspacesAtom, (prev) =>
          prev.map((w) => {
            if (w.id !== wsId || w.sessions.some((s) => s.id === session.id)) return w;
            return { ...w, sessions: insertAt(w.sessions, session, sessionIndex) };
          }),
        );
        if (tabIndex !== -1) {
          appStore.set(sessionTabIdsAtom, (prev) =>
            prev.includes(session.id) ? prev : insertAt(prev, session.id, tabIndex),
          );
        }
        if (wasActive) appStore.set(activeSessionIdAtom, session.id);
      },
    },
  });

  cancelTimer(session.id);
  graceTimers.set(
    session.id,
    setTimeout(() => {
      graceTimers.delete(session.id);
      terminalService.destroy(session.id);
      invoke("delete_session", { sessionId: session.id }).catch(() => {});
    }, DELETE_GRACE_MS),
  );
});

/// Workspace counterpart: hides the workspace (and its session tabs) for
/// the grace window before invoking the destructive delete_workspace.
export const deleteWorkspaceWithGraceAction = atom(null, (get, set, workspace: Workspace) => {
  const workspaces = get(workspacesAtom);
  const wsIndex = workspaces.findIndex((w) => w.id === workspace.id);
  if (wsIndex === -1) return;
  const sessionIds = new Set(workspace.sessions.map((s) => s.id));
  const prevTabIds = get(sessionTabIdsAtom);
  const activeId = get(activeSessionIdAtom);
  const activeInWs = activeId !== null && sessionIds.has(activeId);

  set(workspacesAtom, (prev) => prev.filter((w) => w.id !== workspace.id));
  set(sessionTabIdsAtom, (prev) => prev.filter((id) => !sessionIds.has(id)));
  if (activeInWs) set(activeSessionIdAtom, null);

  const deadline = Date.now() + DELETE_GRACE_MS;
  const toastId = sileo.action({
    title: "Workspace removed",
    description: createElement(CountdownLabel, { sessionName: workspace.name, deadline }),
    duration: DELETE_GRACE_MS,
    fill: "#171717",
    button: {
      title: "Undo",
      onClick: () => {
        cancelTimer(workspace.id);
        sileo.dismiss(toastId);
        appStore.set(workspacesAtom, (prev) =>
          prev.some((w) => w.id === workspace.id) ? prev : insertAt(prev, workspace, wsIndex),
        );
        appStore.set(sessionTabIdsAtom, (prev) => {
          let next = prev;
          for (const [i, id] of prevTabIds.entries()) {
            if (sessionIds.has(id) && !next.includes(id)) next = insertAt(next, id, i);
          }
          return next;
        });
        if (activeInWs && activeId) appStore.set(activeSessionIdAtom, activeId);
      },
    },
  });

  cancelTimer(workspace.id);
  graceTimers.set(
    workspace.id,
    setTimeout(() => {
      graceTimers.delete(workspace.id);
      for (const id of sessionIds) terminalService.destroy(id);
      invoke("delete_workspace", { workspaceId: workspace.id }).catch(() => {});
    }, DELETE_GRACE_MS),
  );
});
