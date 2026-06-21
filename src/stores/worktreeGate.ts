import { atom, type getDefaultStore } from "jotai";
import { invoke } from "@/lib/tauri";
import { toastsAtom } from "./toast";
import {
  workspacesAtom,
  activeSessionIdAtom,
  freshSessionsAtom,
  expandedWorkspaceIdsAtom,
  type Session,
} from "./workspace";

type Store = ReturnType<typeof getDefaultStore>;

/// One pending agent-spawned-worktree request enriched with live resource
/// context. Mirrors the Rust `GateRequestView` (flattened PendingWorktreeRequest
/// + `resources`). `launch_options` is intentionally omitted — the human-facing
/// escalation input (`permission_preset`) is broken out instead.
export type RequestKind =
  | { type: "create" }
  | { type: "resume"; target_session_id: string };

export interface WorktreeRequestView {
  id: string;
  kind: RequestKind;
  requesting_session: string;
  workspace_id: string;
  repo_path: string;
  branch_name: string | null;
  prompt: string;
  agent: string | null;
  permission_preset: string | null;
  /// Verbatim shell prelude — runs as code at spawn; the gate flags any value.
  startup_command: string | null;
  /// Adds bypass to the in-session mode cycle; the gate flags it.
  allow_skip_in_cycle: boolean;
  created_at: number;
  timeout_secs: number;
  resources: {
    worktree_count: number;
    free_disk_bytes: number;
    soft_cap: number;
    over_soft_cap: boolean;
  };
}

export const worktreeRequestsAtom = atom<WorktreeRequestView[]>([]);

/// Re-fetch the pending queue. Called on mount + on every `worktree:request` /
/// `worktree:resolved` event (the events only signal "queue changed").
export async function loadWorktreeRequests(store: Store): Promise<void> {
  try {
    const list = await invoke<WorktreeRequestView[]>("list_worktree_requests");
    store.set(worktreeRequestsAtom, list ?? []);
  } catch {
    // Tolerate failure — the next event re-fetches.
  }
}

/// Approve a request and activate the returned session. Create → a brand-new
/// session is added + marked fresh so it spawns ("new" mode); Resume → the
/// existing session is activated WITHOUT the fresh mark so it resumes
/// ("continue" mode). Either way the backend-queued prompt is its first turn.
export async function approveWorktreeRequest(
  store: Store,
  req: WorktreeRequestView,
  editedPrompt?: string,
  editedBranch?: string,
  editedPreset?: string,
  editedAgent?: string,
): Promise<void> {
  const isResume = req.kind.type === "resume";
  try {
    const session = await invoke<Session>("approve_worktree_request", {
      requestId: req.id,
      editedPrompt: editedPrompt ?? null,
      editedBranch: editedBranch ?? null,
      editedPreset: editedPreset ?? null,
      editedAgent: editedAgent ?? null,
    });
    if (!isResume) {
      // New session: add it to its workspace and mark it fresh ("new" spawn).
      store.set(workspacesAtom, (prev) =>
        prev.map((w) =>
          w.id === session.workspace_id ? { ...w, sessions: [...w.sessions, session] } : w,
        ),
      );
      store.set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
    }
    // Resume: the session already lives in workspacesAtom; NOT marking it fresh
    // makes the terminal activate it in resume ("continue") mode.
    store.set(expandedWorkspaceIdsAtom, (prev) => new Set([...(prev ?? []), session.workspace_id]));
    store.set(activeSessionIdAtom, session.id);
    store.set(toastsAtom, {
      message: isResume ? `Resumed: ${session.name}` : `Worktree session: ${session.name}`,
      description: isResume
        ? "Session revived — the relayed message is queued as its next turn."
        : "Approved — the agent's prompt is queued as its first turn.",
      type: "success",
    });
  } catch (err) {
    store.set(toastsAtom, {
      message: "Approve worktree request failed",
      description: String(err),
      type: "error",
    });
  } finally {
    await loadWorktreeRequests(store);
  }
}

export async function denyWorktreeRequest(store: Store, requestId: string): Promise<void> {
  try {
    await invoke("deny_worktree_request", { requestId });
  } catch (err) {
    store.set(toastsAtom, {
      message: "Deny worktree request failed",
      description: String(err),
      type: "error",
    });
  } finally {
    await loadWorktreeRequests(store);
  }
}
