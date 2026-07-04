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
import { pruneSessionStateAction } from "./sessionScope";
import { pruneConflictSessionAction } from "./conflict";

const DELETE_GRACE_MS = 5_000;
/// Slack between the countdown reaching 0 and the physical deletion, so an
/// Undo clicked at the last visible second still lands before the timer.
const FINALIZE_SLACK_MS = 2_000;

/// A deferred deletion: its grace timer plus the awaitable finalize that
/// performs the real destruction (PTY kill + prune + backend delete). Kept
/// outside Jotai — timers are disposable resources, not state. `finalize` is
/// stored so a shutdown hook can drain it synchronously on app close (BUG-28).
interface PendingDelete {
  timer: ReturnType<typeof setTimeout>;
  finalize: () => Promise<void>;
}
const pending = new Map<string, PendingDelete>();

function cancelTimer(key: string): boolean {
  const entry = pending.get(key);
  if (entry === undefined) return false;
  clearTimeout(entry.timer);
  pending.delete(key);
  return true;
}

/// True while at least one deletion is still inside its grace window.
export function hasPendingDeletes(): boolean {
  return pending.size > 0;
}

/// Runs every still-pending deletion's finalize immediately, awaiting the
/// backend deletes. Wired to the window-close hook so a close during the
/// grace window doesn't strand the delete (leaving the workspace to reappear
/// and its worktree/branch orphaned). Undo is impossible past this point by
/// construction — the app is exiting.
export async function flushPendingDeletes(): Promise<void> {
  const entries = Array.from(pending.values());
  pending.clear();
  await Promise.all(
    entries.map((e) => {
      clearTimeout(e.timer);
      // Never reject: a rejected finalize would bubble to the close hook and
      // stop it from destroying the window.
      return e.finalize().catch(() => {});
    }),
  );
}

function tooLateToast(): void {
  sileo.error({ title: "Too late", description: "Already deleted.", fill: "#171717" });
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
        sileo.dismiss(toastId);
        if (!cancelTimer(session.id)) {
          tooLateToast();
          return;
        }
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

  const finalize = async () => {
    pending.delete(session.id);
    sileo.dismiss(toastId);
    terminalService.destroy(session.id);
    appStore.set(pruneSessionStateAction, session.id);
    appStore.set(pruneConflictSessionAction, session.id);
    await invoke("delete_session", { sessionId: session.id }).catch(() => {});
  };

  cancelTimer(session.id);
  pending.set(session.id, {
    finalize,
    timer: setTimeout(() => void finalize(), DELETE_GRACE_MS + FINALIZE_SLACK_MS),
  });
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
        sileo.dismiss(toastId);
        if (!cancelTimer(workspace.id)) {
          tooLateToast();
          return;
        }
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

  const finalize = async () => {
    pending.delete(workspace.id);
    sileo.dismiss(toastId);
    for (const id of sessionIds) {
      terminalService.destroy(id);
      appStore.set(pruneSessionStateAction, id);
      appStore.set(pruneConflictSessionAction, id);
    }
    await invoke("delete_workspace", { workspaceId: workspace.id }).catch(() => {});
  };

  cancelTimer(workspace.id);
  pending.set(workspace.id, {
    finalize,
    timer: setTimeout(() => void finalize(), DELETE_GRACE_MS + FINALIZE_SLACK_MS),
  });
});
