import { atom } from "jotai";
import {
  activeSessionIdAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
} from "./workspace";

export const addAndActivateSessionAtom = atom(null, (get, set, session: Session) => {
  set(workspacesAtom, (prev) =>
    prev.map((w) =>
      w.id === session.workspace_id && !w.sessions.some((s) => s.id === session.id)
        ? { ...w, sessions: [...w.sessions, session] }
        : w,
    ),
  );
  const expanded = get(expandedWorkspaceIdsAtom) ?? new Set<string>();
  if (!expanded.has(session.workspace_id)) {
    set(expandedWorkspaceIdsAtom, new Set([...expanded, session.workspace_id]));
  }
  set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
  set(activeSessionIdAtom, session.id);
});
