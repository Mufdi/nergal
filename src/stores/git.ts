import { atom } from "jotai";
import { activeSessionIdAtom, sessionTabIdsAtom, workspacesAtom, type Workspace } from "./workspace";
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

/// The five chips of the GitPanel. Each chip owns its full layout; chips are
/// reached via `Shift+←/→` and persist per-workspace.
export type ChipMode = "files" | "history" | "stashes" | "prs" | "conflicts";

export const CHIP_ORDER: ChipMode[] = ["files", "history", "stashes", "prs", "conflicts"];

/// Per-session active chip in the GitPanel. Keyed by sessionId because each
/// chip's data is naturally session-scoped: Files/History/Stashes/Conflicts
/// reflect the worktree of *this* session, and even PRs (workspace data) are
/// a per-session UI choice. Earlier this was per-workspace, which leaked the
/// chip selection across sessions of the same repo — switching from a
/// conflicted session to a clean sibling kept the Conflicts chip showing.
export const gitChipModeAtom = atom<Record<string, ChipMode>>({});

/// A stash entry surfaced by the backend's `git_stash_list`. Mirrors the
/// Rust `StashEntry` struct exactly.
export interface StashEntry {
  index: number;
  message: string;
  branch: string;
  age: string;
}

export interface SelectGitChipParams {
  sessionId: string;
  mode: ChipMode;
}

/// Sets the active chip for a session. Used by consumers that previously
/// opened a tab (PR Viewer's failed-merge → conflicts; conflict list click →
/// conflicts) to instead route to the corresponding chip.
export const selectGitChipAction = atom(
  null,
  (_get, set, params: SelectGitChipParams) => {
    set(gitChipModeAtom, (prev) => ({ ...prev, [params.sessionId]: params.mode }));
  },
);

/// Workspace-scoped cache of `gh pr list` results. The fetch is a network
/// round-trip to GitHub (500-2000ms) that previously fired on every GitPanel
/// mount AND every PrsChip mount AND every 15s. Caching by workspace lets
/// session switches and chip switches read instantly while the background
/// revalidation runs at a relaxed cadence.
export interface PrsCacheEntry {
  data: PrSummary[];
  fetchedAt: number;
}
export const prsCacheMapAtom = atom<Record<string, PrsCacheEntry>>({});

/// TTL for `gh pr list` revalidation. Background refresh fires when the
/// cache is older than this; the UI never blocks on it.
export const PRS_CACHE_TTL_MS = 30_000;

/// Per-session cached count of `git stash list`. Cheap shellout but firing
/// it every 15s × every session × every chip render is wasteful; cache by
/// sessionId and revalidate on `files:modified` events.
export interface StashCountCacheEntry {
  count: number;
  fetchedAt: number;
}
export const stashCountMapAtom = atom<Record<string, StashCountCacheEntry>>({});

export const STASH_CACHE_TTL_MS = 10_000;

/// Cached header data per session: branch + ahead count. Backed by the same
/// `get_session_git_info` shellout chain (rev-parse + diff --shortstat +
/// rev-list). Cheap individually but firing it on every session switch
/// produces a visible "…" flash. Read-from-cache + background refresh keeps
/// the header stable during navigation.
export interface GitHeaderCacheEntry {
  branch: string;
  ahead: number;
  fetchedAt: number;
}
export const gitHeaderMapAtom = atom<Record<string, GitHeaderCacheEntry>>({});

/// Per-session cached PR status (`gh pr view <branch>`). Network call when
/// `gh` is configured; cached so session switches don't re-hit GitHub for
/// the same branch.
export interface PrInfoCacheEntry {
  data: { number: number; title: string; state: string; url: string } | null;
  fetchedAt: number;
}
export const prInfoMapAtom = atom<Record<string, PrInfoCacheEntry>>({});

/// Per-session cached `has_pending_merge` flag. Local git check, but cached
/// for symmetry — keeps the green "in-progress merge" banner stable across
/// switches without a refetch.
export interface PendingMergeCacheEntry {
  pending: boolean;
  fetchedAt: number;
}
export const pendingMergeMapAtom = atom<Record<string, PendingMergeCacheEntry>>({});

/// TTL shared by header + pendingMerge. Short because these reflect local
/// git state that the user can change between switches.
export const SESSION_GIT_TTL_MS = 5_000;

/// TTL for `gh pr view`. Same workspace-network class as `gh pr list`; the
/// PR's title/state rarely flip in seconds.
export const PR_INFO_TTL_MS = 30_000;

/// Cached unified-diff text from `gh pr diff <number>`. Without this every
/// re-open of the same PR within the PrsChip refetched the diff (a network
/// round-trip + a multi-MB parse). Keyed by `${workspaceId}:${prNumber}`
/// so concurrent open PRs each get their own slot.
export interface PrDiffCacheEntry {
  text: string;
  fetchedAt: number;
}
export const prDiffCacheMapAtom = atom<Record<string, PrDiffCacheEntry>>({});

export const PR_DIFF_TTL_MS = 60_000;

/// Per-workspace memory of "which PR was open in the chip". When the user
/// navigates away from the PRs chip and returns, restore the previously
/// opened PR (and via `selectedPrFileAtom`, the file they were viewing
/// inside it) instead of dropping back to the PR list. Cleared explicitly
/// on Backspace ("All PRs" button).
export const activePrInChipMapAtom = atom<Record<string, number | null>>({});

/// Per-PR annotations keyed by `${workspaceId}:${prNumber}`. v3 MVP keeps
/// these in-memory only — they vanish on app restart, which matches the
/// "annotations only exist if they drive Claude" rule from the design doc.
export const prAnnotationsMapAtom = atom<Record<string, PrAnnotation[]>>({});

export function prAnnotationsKey(workspaceId: string, prNumber: number): string {
  return `${workspaceId}:${prNumber}`;
}

/// Files a PR touches, with +/- counts. Populated by PrViewer when it parses
/// the multi-file diff; consumed by the Zen sidebar so it can render a file
/// list for the active PR without re-fetching the diff. Keyed identically to
/// annotations so both stores share one PR identity.
export interface PrFileInfo {
  path: string;
  adds: number;
  removes: number;
}
export const prFilesCacheAtom = atom<Record<string, PrFileInfo[]>>({});

/// The currently-selected file for each PR's PrViewer instance. Lifted here
/// so the in-Zen viewer and the Zen sidebar see the same selection without
/// passing refs through ZenMode.
export const selectedPrFileAtom = atom<Record<string, string | null>>({});

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

