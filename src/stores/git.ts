import { atom } from "jotai";
import { activeSessionIdAtom, sessionTabIdsAtom, workspacesAtom, type Workspace } from "./workspace";
import { openTabAction, expandRightPanelAtom, type TabType } from "./rightPanel";
import { toastsAtom } from "./toast";
import { invoke } from "@/lib/tauri";

export interface GitInfo {
  branch: string;
  dirty: boolean;
  ahead: number;
  lines_added: number;
  lines_removed: number;
}

export interface PrChecks {
  passing: number;
  failing: number;
  pending: number;
  total: number;
}

export interface PrSummary {
  number: number;
  title: string;
  state: string;
  url: string;
  base_ref_name: string;
  head_ref_name: string;
  updated_at: string;
}

export interface PrAnnotation {
  id: string;
  hunkIndex: number;
  text: string;
}

export const gitInfoMapAtom = atom<Record<string, GitInfo>>({});

export const activeGitInfoAtom = atom<GitInfo | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  return get(gitInfoMapAtom)[id] ?? null;
});

export const refreshGitInfoAtom = atom(null, async (_get, set, sessionId: string) => {
  try {
    const info = await invoke<GitInfo>("get_session_git_info", { sessionId });
    set(gitInfoMapAtom, (prev) => ({ ...prev, [sessionId]: info }));
  } catch {
    // silently ignore — session may not have git context
  }
});

export const conflictedFilesMapAtom = atom<Record<string, string[]>>({});

export const activeConflictedFilesAtom = atom<string[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  return get(conflictedFilesMapAtom)[id] ?? [];
});

export const refreshConflictedFilesAtom = atom(null, async (_get, set, sessionId: string) => {
  try {
    const files = await invoke<string[]>("get_conflicted_files", { sessionId });
    set(conflictedFilesMapAtom, (prev) => ({ ...prev, [sessionId]: files }));
  } catch {
    // no-op
  }
});

export const prChecksMapAtom = atom<Record<string, PrChecks | null>>({});

export const activePrChecksAtom = atom<PrChecks | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  return get(prChecksMapAtom)[id] ?? null;
});

export type GitSidebarMode = "files" | "prs";

/// Per-workspace toggle: show staged/unstaged/untracked vs. the workspace's
/// PRs list in the GitPanel sidebar. Default "files".
export const gitSidebarModeAtom = atom<Record<string, GitSidebarMode>>({});

/// The five chips of the GitPanel. Each chip owns its full layout; chips are
/// reached via `Shift+←/→` and persist per-workspace.
export type ChipMode = "files" | "history" | "stashes" | "prs" | "conflicts";

export const CHIP_ORDER: ChipMode[] = ["files", "history", "stashes", "prs", "conflicts"];

/// Per-workspace active chip in the GitPanel.
export const gitChipModeAtom = atom<Record<string, ChipMode>>({});

/// Whether the GitPanel is in expansion mode (full right-panel width). Toggled
/// via `Ctrl+Shift+0` from anywhere in the GitPanel.
export const gitPanelExpandedAtom = atom<boolean>(false);

/// The currently focused conflict file inside the Conflicts chip's file
/// picker. Updated when a consumer (e.g. PrViewer's failed-merge path or the
/// GitPanel inline conflicts list) routes the user to the chip with a
/// specific file pre-selected.
export const selectedConflictFileAtom = atom<string | null>(null);

/// A stash entry surfaced by the backend's `git_stash_list`. Mirrors the
/// Rust `StashEntry` struct exactly.
export interface StashEntry {
  index: number;
  message: string;
  branch: string;
  age: string;
}

export interface SelectGitChipParams {
  workspaceId: string;
  mode: ChipMode;
}

/// Sets the active chip for a workspace. Used by consumers that previously
/// opened a tab (PR Viewer's failed-merge → conflicts; conflict list click →
/// conflicts) to instead route to the corresponding chip.
export const selectGitChipAction = atom(
  null,
  (_get, set, params: SelectGitChipParams) => {
    set(gitChipModeAtom, (prev) => ({ ...prev, [params.workspaceId]: params.mode }));
  },
);

/// Per-PR annotations keyed by `${workspaceId}:${prNumber}`. v3 MVP keeps
/// these in-memory only — they vanish on app restart, which matches the
/// "annotations only exist if they drive Claude" rule from the design doc.
export const prAnnotationsMapAtom = atom<Record<string, PrAnnotation[]>>({});

export function prAnnotationsKey(workspaceId: string, prNumber: number): string {
  return `${workspaceId}:${prNumber}`;
}

/// Tab id format for a PR Viewer tab. Stable per (workspace, PR) so reopening
/// the same PR focuses the existing tab instead of duplicating it.
export function prTabId(workspaceId: string, prNumber: number): string {
  return `pr:${workspaceId}:${prNumber}`;
}

export interface OpenPrTabParams {
  workspaceId: string;
  pr: PrSummary;
}

export interface TransitionAfterCleanupParams {
  deletedSessionId: string;
  workspaceId: string;
  warnings: string[];
  archivedPlansPath?: string | null;
}

/// Post-cleanup transition: removes the deleted session from open tabs,
/// refreshes the workspaces atom from the backend (so the deleted row is
/// gone), then either switches to the most-recently-updated remaining
/// session in the same workspace, or clears `activeSessionIdAtom` when
/// the workspace is empty (right panel renders the empty state on its
/// own when there's no active session).
export const transitionAfterCleanupAction = atom(
  null,
  async (get, set, params: TransitionAfterCleanupParams) => {
    const { deletedSessionId, workspaceId, warnings, archivedPlansPath } = params;

    set(sessionTabIdsAtom, (prev) => prev.filter((id) => id !== deletedSessionId));

    let workspaces: Workspace[];
    try {
      workspaces = await invoke<Workspace[]>("get_workspaces");
      set(workspacesAtom, workspaces);
    } catch {
      workspaces = get(workspacesAtom).map((w) => ({
        ...w,
        sessions: w.sessions.filter((s) => s.id !== deletedSessionId),
      }));
      set(workspacesAtom, workspaces);
    }

    const ws = workspaces.find((w) => w.id === workspaceId);
    const candidates = (ws?.sessions ?? [])
      .filter((s) => s.id !== deletedSessionId && s.status !== "completed")
      .sort((a, b) => b.updated_at - a.updated_at);
    const next = candidates[0] ?? null;

    const archiveSuffix = archivedPlansPath ? ` Plans archived to ${archivedPlansPath}` : "";
    const warningSuffix = warnings.length > 0
      ? ` (${warnings.length} warning${warnings.length === 1 ? "" : "s"})`
      : "";

    if (next) {
      set(activeSessionIdAtom, next.id);
      set(toastsAtom, {
        message: "Session cleaned up",
        description: `Switched to "${next.name}".${archiveSuffix}${warningSuffix}`,
        type: "success",
      });
    } else {
      const current = get(activeSessionIdAtom);
      if (current === deletedSessionId) {
        set(activeSessionIdAtom, null);
      }
      set(toastsAtom, {
        message: "Session cleaned up",
        description: `Workspace empty. Press Ctrl+N to start a new session.${archiveSuffix}${warningSuffix}`,
        type: "success",
      });
    }
  },
);

/// Opens (or focuses) a PR Viewer tab. Multiple PR tabs can coexist — they're
/// keyed by `(workspaceId, prNumber)` so each PR gets its own tab.
export const openPrTabAction = atom(
  null,
  (_get, set, params: OpenPrTabParams) => {
    const { workspaceId, pr } = params;
    const id = prTabId(workspaceId, pr.number);
    set(openTabAction, {
      tab: {
        id,
        type: "pr" as TabType,
        label: `PR #${pr.number}`,
        data: {
          workspaceId,
          prNumber: pr.number,
          title: pr.title,
          state: pr.state,
          url: pr.url,
          baseRefName: pr.base_ref_name,
          headRefName: pr.head_ref_name,
          updatedAt: pr.updated_at,
        },
      },
      isPinned: true,
    });
    set(expandRightPanelAtom, (prev: number) => prev + 1);
  },
);

