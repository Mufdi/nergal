import { atom } from "jotai";
import type { SessionInfo, CostSummary, WorkspaceInfo } from "@/lib/types";

export const sessionsAtom = atom<SessionInfo[]>([]);
export const activeSessionIndexAtom = atom<number>(0);

export const activeSessionAtom = atom<SessionInfo | null>((get) => {
  const sessions = get(sessionsAtom);
  const index = get(activeSessionIndexAtom);
  return sessions[index] ?? null;
});

export const sessionModeAtom = atom<string>("idle");

/// Active terminal PTY id — set by useTerminal, read by PlanPanel for approve/reject
export const terminalIdAtom = atom<string | null>(null);

export const workspacesAtom = atom<WorkspaceInfo[]>((get) => {
  const sessions = get(sessionsAtom);
  const grouped = new Map<string, SessionInfo[]>();
  for (const session of sessions) {
    const key = session.cwd ?? "unknown";
    const list = grouped.get(key) ?? [];
    list.push(session);
    grouped.set(key, list);
  }
  const workspaces: WorkspaceInfo[] = [];
  for (const [path, wsSessions] of grouped) {
    const parts = path.split("/");
    workspaces.push({
      path,
      name: parts[parts.length - 1] || path,
      sessions: wsSessions,
    });
  }
  return workspaces;
});

export const costSummaryAtom = atom<CostSummary>({
  input_tokens: 0,
  output_tokens: 0,
  cache_read: 0,
  cache_write: 0,
  total_usd: 0,
});
