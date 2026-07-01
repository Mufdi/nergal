import { atom } from "jotai";
import type { CostSummary } from "@/lib/types";

/// Mirrors `PermissionPreset` in src-tauri/src/models.rs (kebab-case wire
/// form). One mode per session — CC's `--dangerously-skip-permissions` is
/// the documented equivalent of `--permission-mode bypassPermissions`, so
/// "bypass" is a mode like the rest, never combinable with plan/accept.
export type PermissionPreset = "default" | "plan" | "accept-edits" | "auto" | "bypass";

/// Mirrors `LaunchOptions` in src-tauri/src/models.rs.
export interface LaunchOptions {
  permission_preset: PermissionPreset;
  /// CC `--allow-dangerously-skip-permissions`: adds bypass to the Shift+Tab
  /// cycle without starting in it. Composes with any non-bypass preset.
  allow_skip_in_cycle: boolean;
  startup_command: string | null;
}

/// Mirrors `EnvShellDef` in src-tauri/src/models.rs: a long-running command
/// that gets its own quake shell instead of blocking the agent terminal.
export interface EnvShellDef {
  label: string;
  command: string;
  /// Working directory when it differs from the session cwd (front/back
  /// split repos). `~` expands; relative paths resolve against the
  /// workspace root at spawn.
  cwd?: string | null;
}

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
  /// Absolute vault-note paths pinned to this session (obsidian-context
  /// -injection). Seeds the chip's initial count; runtime mutations live in
  /// `pinnedNotesMapAtom`.
  pinned_note_paths?: string[];
  /// Launch options chosen at creation; re-applied on resume by the backend.
  launch_options?: LaunchOptions | null;
  /// Environment shells (quake): auto-run at creation, pre-filled on re-open.
  env_shells?: EnvShellDef[];
  /// The single bound ClickUp task (write-back target + session-tab chip).
  /// Seeds the initial value; runtime mutations live in `clickupBindingMapAtom`.
  active_clickup_task_id?: string | null;
  /// ClickUp tasks pinned as context-only. Seeds the initial value; runtime
  /// mutations live in `clickupPinsMapAtom`.
  pinned_clickup_task_ids?: string[];
  /// The single bound Linear issue (write-back target + session-tab chip).
  /// Seeds the initial value; runtime mutations live in `linearBindingMapAtom`.
  active_linear_issue_id?: string | null;
  /// Linear issues pinned as context-only. Seeds the initial value; runtime
  /// mutations live in `linearPinsMapAtom`.
  pinned_linear_issue_ids?: string[];
}

export interface Workspace {
  id: string;
  name: string;
  repo_path: string;
  sessions: Session[];
  created_at: number;
  is_git: boolean;
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

/// The workspace the user is currently working in even when no session is
/// active (set by the sidebar on open/expand). Settings reads it so a brand-new
/// workspace's forms don't silently bind to workspaces[0].
export const selectedWorkspaceIdAtom = atom<string | null>(null);

/// Defaults to true while no workspace is resolved yet, so git UI doesn't
/// flash hidden during boot.
export const activeWorkspaceIsGitAtom = atom<boolean>((get) => {
  return get(activeWorkspaceAtom)?.is_git ?? true;
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

/// Draft state for the Settings → Paths OpenSpec field. Held in an atom (not
/// local component state) so the global Settings Save button can flush it
/// without a dedicated Apply button. `baseline` is the value as loaded, so
/// Save can skip a no-op write.
export interface OpenspecDirDraft {
  workspaceId: string;
  value: string;
  defaultDir: string;
  baseline: string;
}
export const openspecDirDraftAtom = atom<OpenspecDirDraft | null>(null);

/// Draft state for the Settings → Paths Plans Dir field. Held in an atom so
/// the global Settings Save button can flush it without a dedicated Apply button.
export interface PlansDirDraft {
  workspaceId: string;
  value: string;
  defaultDir: string;
  baseline: string;
}
export const plansDirDraftAtom = atom<PlansDirDraft | null>(null);

// Session tabs — ordered list of session IDs open as tabs in the TopBar
export const sessionTabIdsAtom = atom<string[]>([]);

/// Persisted across sidebar collapse/expand cycles so re-expanding doesn't
/// reset everything but the first workspace. `null` means "not yet hydrated"
/// (workspaces load auto-expands the first); subsequent edits store the set.
export const expandedWorkspaceIdsAtom = atom<Set<string> | null>(null);

// Session-scoped state maps
export const costMapAtom = atom<Record<string, CostSummary>>({});
export const modeMapAtom = atom<Record<string, string>>({});
export const cwdMapAtom = atom<Record<string, string>>({});
export const freshSessionsAtom = atom<Set<string>>(new Set<string>());
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
