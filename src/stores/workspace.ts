import { atom } from "jotai";
import type { CostSummary } from "@/lib/types";

export interface Session {
  id: string;
  name: string;
  workspace_id: string;
  worktree_path: string | null;
  worktree_branch: string | null;
  merge_target: string | null;
  status: "idle" | "running" | "needs_attention" | "completed";
  created_at: number;
  updated_at: number;
}

export interface Workspace {
  id: string;
  name: string;
  repo_path: string;
  sessions: Session[];
  created_at: number;
}

// Core atoms
export const workspacesAtom = atom<Workspace[]>([]);
export const activeSessionIdAtom = atom<string | null>(null);

// Derived atoms
export const activeSessionAtom = atom<Session | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  const workspaces = get(workspacesAtom);
  for (const ws of workspaces) {
    for (const s of ws.sessions) {
      if (s.id === id) return s;
    }
  }
  return null;
});

export const activeWorkspaceAtom = atom<Workspace | null>((get) => {
  const session = get(activeSessionAtom);
  if (!session) return null;
  const workspaces = get(workspacesAtom);
  return workspaces.find((w) => w.id === session.workspace_id) ?? null;
});

// Session tabs — ordered list of session IDs open as tabs in the TopBar
export const sessionTabIdsAtom = atom<string[]>([]);

// Session-scoped state maps
export const costMapAtom = atom<Record<string, CostSummary>>({});
export const modeMapAtom = atom<Record<string, string>>({});
export const cwdMapAtom = atom<Record<string, string>>({});
export const freshSessionsAtom = atom<Set<string>>(new Set<string>());
export const showCompletedAtom = atom(false);
export const sessionLaunchModeAtom = atom<Record<string, "new" | "continue" | "resume_pick">>({});


const defaultCost: CostSummary = {
  input_tokens: 0,
  output_tokens: 0,
  cache_read: 0,
  cache_write: 0,
  total_usd: 0,
};

// Derived active-session state
export const activeCostAtom = atom<CostSummary>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return defaultCost;
  return get(costMapAtom)[id] ?? defaultCost;
});

export const activeModeAtom = atom<string>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return "idle";
  return get(modeMapAtom)[id] ?? "idle";
});

export const activeCwdAtom = atom<string | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  return get(cwdMapAtom)[id] ?? null;
});
