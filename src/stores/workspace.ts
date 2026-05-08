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
  /// Identifier of the agent adapter that owns this session. Defaults to
  /// "claude-code" for legacy rows. Set to "opencode" / "codex" / "pi" for
  /// non-CC sessions.
  agent_id?: string;
  /// Wire-form capability bitset emitted by the backend on session creation
  /// so the frontend can gate UI synchronously without a separate fetch.
  agent_capabilities?: string[];
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

/// Reverse index sessionId → workspaceId. Lets callers (e.g. GitPanel) skip
/// the O(workspaces × sessions) scan that previously ran on every render
/// when they only needed the parent workspace of a known session.
export const sessionToWorkspaceMapAtom = atom<Record<string, string>>((get) => {
  const map: Record<string, string> = {};
  for (const ws of get(workspacesAtom)) {
    for (const s of ws.sessions) map[s.id] = ws.id;
  }
  return map;
});

// Session tabs — ordered list of session IDs open as tabs in the TopBar
export const sessionTabIdsAtom = atom<string[]>([]);

// Session-scoped state maps
export const costMapAtom = atom<Record<string, CostSummary>>({});
export const modeMapAtom = atom<Record<string, string>>({});
export const cwdMapAtom = atom<Record<string, string>>({});
export const freshSessionsAtom = atom<Set<string>>(new Set<string>());
export const showCompletedAtom = atom(false);
export const sessionLaunchModeAtom = atom<Record<string, "new" | "continue">>({});


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

// -- Agent status snapshot (agent-agnostic) --

export interface AgentStatus {
  agent_id: string | null;
  model_id: string | null;
  model_name: string | null;
  session_started_at: number | null;
  context_used_pct: number | null;
  context_window_size: number | null;
  rate_5h_pct: number | null;
  rate_5h_resets_at: number | null;
  rate_7d_pct: number | null;
  rate_7d_resets_at: number | null;
  effort_level: string | null;
}

const defaultAgentStatus: AgentStatus = {
  agent_id: null,
  model_id: null,
  model_name: null,
  session_started_at: null,
  context_used_pct: null,
  context_window_size: null,
  rate_5h_pct: null,
  rate_5h_resets_at: null,
  rate_7d_pct: null,
  rate_7d_resets_at: null,
  effort_level: null,
};

export const agentStatusMapAtom = atom<Record<string, AgentStatus>>({});

export const activeAgentStatusAtom = atom<AgentStatus>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return defaultAgentStatus;
  return get(agentStatusMapAtom)[id] ?? defaultAgentStatus;
});
