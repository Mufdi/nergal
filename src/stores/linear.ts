import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";
import { confirm as swalConfirm } from "@/lib/swal";
import { focusZoneAtom } from "./shortcuts";
import { toastsAtom } from "./toast";
import {
  activeSessionIdAtom,
  activeWorkspaceAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
} from "./workspace";

// ── Writeback: optimistic overlay (task 6.1 / Decision 1) ──

/// Per-field key for the overlay atom.
export type LinearOverlayKey = `${string}:${string}`; // `${issueId}:${field}`

/// A pending edit sent to the API but not yet ack'd by a mirror reconcile.
/// The overlay is volatile — it is never written to SQLite.
export interface LinearOverlayEntry {
  value: string | null;
}

/// In-memory optimistic overlay keyed by `${issueId}:${field}`. Cleared on
/// API success (mirror reconcile will carry the truth); reverted on failure.
export const linearOverlayAtom = atom<Record<LinearOverlayKey, LinearOverlayEntry>>({});

/// Set an overlay entry for `issueId:field`.
export function setLinearOverlayEntry(
  set: (updater: (prev: Record<LinearOverlayKey, LinearOverlayEntry>) => Record<LinearOverlayKey, LinearOverlayEntry>) => void,
  issueId: string,
  field: string,
  value: string | null,
) {
  const key: LinearOverlayKey = `${issueId}:${field}`;
  set((prev) => ({ ...prev, [key]: { value } }));
}

/// Clear an overlay entry (on API success or failure).
export function clearLinearOverlayEntry(
  set: (updater: (prev: Record<LinearOverlayKey, LinearOverlayEntry>) => Record<LinearOverlayKey, LinearOverlayEntry>) => void,
  issueId: string,
  field: string,
) {
  const key: LinearOverlayKey = `${issueId}:${field}`;
  set((prev) => {
    const next = { ...prev };
    delete next[key];
    return next;
  });
}

// ── Closure offer atom (task 6.1) ──

export interface LinearClosureOffer {
  issueId: string;
  sessionId: string;
  prUrl?: string;
}

/// Non-null when the Linear closure prompt is open. Raised by ship-success (prUrl
/// set) or by the manual "Close out issue" verb (no prUrl).
export const linearClosureOfferAtom = atom<LinearClosureOffer | null>(null);

// ── Closed-out set (task 6.1 / 6.3) ──

/// Issue ids that have been closed out locally. Drives the panel/detail badge.
/// Uses a Set for O(1) membership checks — never written before a server ack.
export const linearClosedOutAtom = atom<Set<string>>(new Set<string>());

// ── WorkflowState shape (returned by linear_read_team_states) ──

export interface WorkflowStateView {
  id: string;
  name: string;
  type: string;
  color?: string;
  position: number;
}

type Store = ReturnType<typeof getDefaultStore>;

// ── Backend view shapes ──

export interface IssueView {
  id: string;
  identifier?: string;
  teamId: string;
  title: string;
  description?: string;
  priority: number;
  stateId?: string;
  stateName?: string;
  stateType?: string;
  stateColor?: string;
  statePosition?: number;
  assigneeId?: string;
  assigneeName?: string;
  assigneeAvatarUrl?: string;
  projectId?: string;
  projectName?: string;
  cycleId?: string;
  cycleName?: string;
  parentId?: string;
  updatedAt?: number;
  createdAt?: number;
  dueDate?: number;
  url?: string;
  estimate?: number;
  estimationType?: string;
  stale: boolean;
  labels: { id: string; name: string; color?: string }[];
}

export interface LinearAttachment {
  id: string;
  title?: string;
  subtitle?: string;
  url: string;
}

export interface LinearRelation {
  relationType: string;
  relatedId: string;
  relatedIdentifier?: string;
  relatedTitle?: string;
}

export interface LinearActivityEntry {
  id: string;
  createdAt?: number;
  actor?: string;
  kind: "created" | "state" | "assignee" | "label" | "cycle" | "priority";
  from?: string;
  to?: string;
  added: string[];
  removed: string[];
}

export interface LinearIssueDetail {
  comments: LinearComment[];
  attachments: LinearAttachment[];
  relations: LinearRelation[];
  activity: LinearActivityEntry[];
}

export interface TeamView {
  id: string;
  name: string;
  key: string;
}

export interface LinearWorkspace {
  orgId: string;
  name: string;
  urlKey?: string;
  active: boolean;
}

export interface SyncStatus {
  state: "idle" | "no_key" | "needs_team" | "syncing" | "ok" | "error";
  viewerId?: string;
  viewerName?: string;
  selectedTeamIds: string[];
  lastSync?: number;
  baselineDone: boolean;
  keyOnDisk: boolean;
  error?: string;
}

export interface LinearComment {
  id: string;
  body?: string;
  createdAt?: number;
  author?: string;
  parentId?: string | null;
}

// ── Data atoms ──

export const linearSyncStatusAtom = atom<SyncStatus | null>(null);

/// Gates TopBar entry: configured when we have a key and are past no_key/idle.
export const linearConfiguredAtom = atom((get) => {
  const status = get(linearSyncStatusAtom);
  return status !== null && status.state !== "no_key" && status.state !== "idle";
});

export const linearIssuesAtom = atom<IssueView[]>([]);
export const linearTeamsAtom = atom<TeamView[]>([]);
export const linearKeyOnDiskAtom = atom(false);

// ── UI prefs ──

export type LinearGroupBy = "state" | "project" | "assignee" | "cycle";
export const GROUP_BY_ORDER: LinearGroupBy[] = ["state", "project", "assignee", "cycle"];

/// Sort applied to issues within each group (and to nested sub-issue siblings).
/// Linear priority int: 0=none,1=urgent,2=high,3=medium,4=low — "priority
/// ascending" means urgent first, none last (mirrors ClickUp's sortKey null-last).
export type LinearSortField = "updated" | "created" | "priority" | "due";
export interface LinearSort {
  field: LinearSortField;
  dir: "asc" | "desc";
}
export const linearSortAtom = atom<LinearSort>({ field: "updated", dir: "desc" });

/// null = "Todos" (all teams).
export const linearTeamFilterAtom = atom<string | null>(null);
export const linearGroupByAtom = atom<LinearGroupBy>("state");
/// Default true: show only issues assigned to the viewer.
export const linearAssignedToMeAtom = atom(true);
/// When true, completed/canceled/duplicate issues show (local filter, no fetch).
/// Default on — the user wants terminal issues visible (they ARE mirrored).
export const linearShowCompletedAtom = atom(true);

/// Issue currently open in the floating detail module (null = closed).
export const linearDetailIssueIdAtom = atom<string | null>(null);

/// Active label filter: set of label ids. Empty set = no filter.
export const linearLabelFilterAtom = atom<ReadonlySet<string>>(new Set<string>());

/// Active project filter — null = "Todos" (group all by project); a project id = filter + group by state.
export const linearProjectFilterAtom = atom<string | null>(null);

/// Copy an issue identifier to the OS clipboard + toast. Routes through the
/// robust terminal_clipboard_write (Wayland plugin stalls async) matching
/// copyTaskIdAction in clickup.ts.
export const copyLinearIssueAction = atom(null, (_get, set, identifier: string) => {
  void invoke("terminal_clipboard_write", { text: identifier })
    .then(() =>
      set(toastsAtom, { message: "Issue ID copied", description: identifier, type: "success" }),
    )
    .catch((err) =>
      set(toastsAtom, { message: "Copy failed", description: String(err), type: "error" }),
    );
});

// ── Session ↔ issue binding + issue-to-agent verbs (linear-agent-integration) ──

/// session_id → active issue id (command results are authoritative; absent key
/// falls back to the Session row). Mirrors `clickupBindingMapAtom`.
export const linearBindingMapAtom = atom<Record<string, string | null>>({});

/// session_id → pinned issue ids (command results are authoritative; absent key
/// falls back to the Session row).
export const linearPinsMapAtom = atom<Record<string, string[]>>({});

/// Pending send-as-prompt confirmation (the send auto-submits a turn, so the
/// user reviews the composed block first). null = dialog closed.
export const linearSendConfirmAtom = atom<{ sessionId: string; issueId: string } | null>(null);

/// One-shot flag: when a worktree spawn closes the floating detail, the detail's
/// close-effect must NOT restore focus to the panel — focus belongs to the new
/// session's terminal. Module-level (not an atom) so the store action can set it
/// and the component effect consumes it (mirrors ClickUp's suppressCloseFocusRef,
/// which is component-local because ClickUp's convert-to-tab is the only setter).
let _suppressDetailCloseFocus = false;
export function suppressLinearDetailCloseFocus(): void {
  _suppressDetailCloseFocus = true;
}
export function consumeLinearDetailCloseFocusSuppress(): boolean {
  const v = _suppressDetailCloseFocus;
  _suppressDetailCloseFocus = false;
  return v;
}

/// Shared binding resolution for surfaces that render per-session rows (session
/// tabs): runtime map wins, Session row seeds.
export function resolveActiveLinearIssue(
  map: Record<string, string | null>,
  session: Pick<Session, "id" | "active_linear_issue_id">,
): string | null {
  const runtime = map[session.id];
  return runtime !== undefined ? runtime : (session.active_linear_issue_id ?? null);
}

function findSession(workspaces: { sessions: Session[] }[], sessionId: string): Session | null {
  for (const ws of workspaces) {
    for (const s of ws.sessions) {
      if (s.id === sessionId) return s;
    }
  }
  return null;
}

/// Public binding resolver keyed by sessionId — wraps the module-private
/// findSession so ShipDialog.tsx can resolve the shipped session's bound issue
/// without reading the active session (which may differ from the shipped one).
/// Mirrors resolveActiveClickUpTaskById in clickup.ts.
export function resolveActiveLinearIssueById(
  workspaces: { sessions: Session[] }[],
  bindingMap: Record<string, string | null>,
  sessionId: string,
): string | null {
  const runtime = bindingMap[sessionId];
  if (runtime !== undefined) return runtime;
  return findSession(workspaces, sessionId)?.active_linear_issue_id ?? null;
}

export const activeSessionLinearIssueAtom = atom<string | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  const runtime = get(linearBindingMapAtom)[id];
  if (runtime !== undefined) return runtime;
  return findSession(get(workspacesAtom), id)?.active_linear_issue_id ?? null;
});

export const activeSessionLinearPinsAtom = atom<string[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  const runtime = get(linearPinsMapAtom)[id];
  if (runtime !== undefined) return runtime;
  return findSession(get(workspacesAtom), id)?.pinned_linear_issue_ids ?? [];
});

/// Unbind/unpin affect future spawns/resumes only — context already in a
/// running agent's window is not retracted (design Decision 7).
export const FUTURE_SPAWNS_HINT =
  "Affects future spawns/resumes — the live window keeps its context.";

/// Issue verbs shared by the panel rows, the floating-detail toolbar and the
/// contextual keys (S/W/P/B/R inside the linear zone).
export interface LinearIssueActions {
  send: (issueId: string) => void;
  spawn: (issueId: string) => void;
  togglePin: (issueId: string) => void;
  toggleBind: (issueId: string) => void;
  reinject: (issueId: string) => void;
}

/// Action labels advertise the contextual key (tooltip-as-source-of-truth).
export const LINEAR_ACTION_LABELS = {
  send: "Send as prompt (S)",
  spawn: "Spawn worktree session (W)",
  pin: "Attach as context (P)",
  unpin: `Unpin (P) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
  bind: "Bind as active issue (B)",
  unbind: `Unbind (B) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
  reinject: "Re-inject into the live session (R)",
  closeOut: "Close out issue (C) — mark done & unbind",
} as const;

/// Linear issue titles are multi-writer input and swal bodies render as HTML.
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/// Human label for an issue: identifier (ENG-123) when present, else the title.
function issueDisplay(get: Store["get"], issueId: string): string {
  const issue = get(linearIssuesAtom).find((i) => i.id === issueId);
  if (!issue) return issueId;
  return issue.identifier ?? issue.title;
}

export const requestSendIssueAction = atom(null, (get, set, issueId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Send as prompt",
      description: "No active session to send to.",
      type: "info",
    });
    return;
  }
  set(linearSendConfirmAtom, { sessionId, issueId });
});

export const togglePinIssueAction = atom(null, async (get, set, issueId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Attach as context",
      description: "No active session to attach to.",
      type: "info",
    });
    return;
  }
  const pinned = get(activeSessionLinearPinsAtom).includes(issueId);
  try {
    const ids = await invoke<string[]>(pinned ? "linear_unpin_issue" : "linear_pin_issue", {
      sessionId,
      issueId,
    });
    set(linearPinsMapAtom, (prev) => ({ ...prev, [sessionId]: ids }));
    if (!pinned) {
      // Deliver to the live agent now (mirrors the ClickUp pin); the persisted
      // pin still seeds the next spawn/resume. No-op without a live PTY.
      void invoke("linear_reinject_issue", { sessionId, issueId }).catch(() => {});
    }
    set(
      toastsAtom,
      pinned
        ? { message: "Issue unpinned", description: FUTURE_SPAWNS_HINT, type: "info" }
        : {
            message: "Issue attached as context",
            description:
              "Attached + injected into the live session — also persists for future restarts.",
            type: "success",
          },
    );
  } catch (err) {
    set(toastsAtom, {
      message: pinned ? "Unpin failed" : "Pin failed",
      description: String(err),
      type: "error",
    });
  }
});

export const performBindIssueAction = atom(
  null,
  async (_get, set, args: { sessionId: string; issueId: string }) => {
    try {
      await invoke("linear_bind_issue", args);
      set(linearBindingMapAtom, (prev) => ({ ...prev, [args.sessionId]: args.issueId }));
      // Deliver the brief to the live agent now, SUBMITTED as a turn (binding is
      // the deliberate "work on this" act). Still seeds future spawns/resumes.
      // No-op without a live PTY.
      void invoke("linear_reinject_issue", { ...args, submit: true }).catch(() => {});
      set(toastsAtom, {
        message: "Bound as active issue",
        description:
          "Write-back target for this session (shown in the tab chip) — brief submitted to the live session; also persists for future spawns/resumes.",
        type: "success",
      });
    } catch (err) {
      set(toastsAtom, { message: "Bind failed", description: String(err), type: "error" });
    }
  },
);

export const unbindIssueAction = atom(null, async (get, set) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;
  try {
    await invoke("linear_unbind_issue", { sessionId });
    set(linearBindingMapAtom, (prev) => ({ ...prev, [sessionId]: null }));
    set(toastsAtom, { message: "Issue unbound", description: FUTURE_SPAWNS_HINT, type: "info" });
  } catch (err) {
    set(toastsAtom, { message: "Unbind failed", description: String(err), type: "error" });
  }
});

/// Bind toggle: same issue → unbind; different active issue → confirm the
/// replacement (Decision 2 rebind rule); no active issue → bind directly.
export const requestBindIssueAction = atom(null, async (get, set, issueId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Bind issue",
      description: "No active session to bind to.",
      type: "info",
    });
    return;
  }
  const current = get(activeSessionLinearIssueAtom);
  if (current === issueId) {
    await set(unbindIssueAction);
    return;
  }
  if (current) {
    const confirmed = await swalConfirm({
      title: "Replace active issue?",
      body: `This session is bound to "${escapeHtml(issueDisplay(get, current))}". Binding "${escapeHtml(issueDisplay(get, issueId))}" replaces it as the write-back target. The replaced issue stays in Linear.`,
      confirmLabel: "Replace",
      kind: "question",
    });
    if (!confirmed) return;
  }
  await set(performBindIssueAction, { sessionId, issueId });
});

export const spawnWorktreeWithIssueAction = atom(null, async (get, set, issueId: string) => {
  const workspace = get(activeWorkspaceAtom);
  if (!workspace) {
    set(toastsAtom, {
      message: "Spawn worktree",
      description: "No active workspace — open a session first.",
      type: "info",
    });
    return;
  }
  if (!workspace.is_git) {
    set(toastsAtom, {
      message: "Spawn worktree",
      description: "Not a git workspace — worktrees need a git repository.",
      type: "info",
    });
    return;
  }
  try {
    const session = await invoke<Session>("linear_spawn_worktree_with_issue", {
      workspaceId: workspace.id,
      issueId,
    });
    set(workspacesAtom, (prev) =>
      prev.map((w) => (w.id === workspace.id ? { ...w, sessions: [...w.sessions, session] } : w)),
    );
    set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
    set(expandedWorkspaceIdsAtom, (prev) => new Set([...(prev ?? []), workspace.id]));
    // Activation spawns the PTY, which consumes the backend-queued initial
    // prompt — same flow as the ClickUp worktree verb.
    set(activeSessionIdAtom, session.id);
    // When spawned from the floating detail, close it and send focus to the new
    // session's prompt (not back to the panel). The suppress flag stops the
    // detail's close-effect from yanking focus to the panel; setting the focus
    // zone to "terminal" makes TerminalManager focus the freshly active PTY.
    if (get(linearDetailIssueIdAtom) !== null) {
      suppressLinearDetailCloseFocus();
      set(linearDetailIssueIdAtom, null);
      set(focusZoneAtom, "terminal");
    }
    set(toastsAtom, {
      message: `Worktree session: ${session.name}`,
      description: "Issue bound and queued as the initial prompt.",
      type: "success",
    });
  } catch (err) {
    set(toastsAtom, { message: "Spawn worktree failed", description: String(err), type: "error" });
  }
});

/// Explicit refresh of a live session's issue context (the "stale injected
/// context" risk, mirroring the Obsidian hot-reload rule): recompose +
/// bracketed paste into the live PTY, never automatic, never submits.
export const reinjectIssueAction = atom(null, async (get, set, issueId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Reinject context",
      description: "No active session to reinject into.",
      type: "info",
    });
    return;
  }
  try {
    await invoke("linear_reinject_issue", { sessionId, issueId });
    set(toastsAtom, {
      message: "Context re-injected into session",
      description: "Seeded as context, not submitted.",
      type: "success",
    });
  } catch (err) {
    set(toastsAtom, { message: "Reinject failed", description: String(err), type: "error" });
  }
});

// ── Refresh + bootstrap ──

export async function refreshLinearMirror(store: Store): Promise<void> {
  try {
    const [issues, teams, closedOutIds] = await Promise.all([
      invoke<IssueView[]>("linear_read_issues", {}),
      invoke<TeamView[]>("linear_read_teams"),
      invoke<string[]>("linear_read_closed_out"),
    ]);
    store.set(linearIssuesAtom, issues);
    store.set(linearTeamsAtom, teams);
    store.set(linearClosedOutAtom, new Set(closedOutIds));
  } catch (err) {
    console.warn("[linear] mirror read failed:", err);
  }
}

export async function setupLinearListeners(store: Store): Promise<UnlistenFn[]> {
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<SyncStatus>("linear:sync-status", (payload) => {
      store.set(linearSyncStatusAtom, payload);
    }),
  );

  unlisteners.push(
    await listen<null>("linear:changed", () => {
      void refreshLinearMirror(store);
    }),
  );

  unlisteners.push(
    await listen<string[]>("linear:assigned", (titles) => {
      if (!Array.isArray(titles) || titles.length === 0) return;
      store.set(toastsAtom, {
        message: titles.length === 1 ? "New Linear issue assigned" : `${titles.length} new Linear issues assigned`,
        description: titles.length === 1 ? titles[0] : titles.join(" · "),
        type: "info",
      });
    }),
  );

  // Scalar write-conflict: remote superseded a local edit while we held an
  // overlay entry (Decision 3). Warn via toast.
  unlisteners.push(
    await listen<{ issue_id: string; field: string; your_value: string; remote_value: string }>(
      "linear:write-conflict",
      (payload) => {
        store.set(toastsAtom, {
          message: "Linear write conflict",
          description: `${payload.field}: your change was superseded by a remote edit (remote: "${payload.remote_value}")`,
          type: "info",
        });
      },
    ),
  );

  try {
    const status = await invoke<SyncStatus>("linear_sync_status");
    store.set(linearSyncStatusAtom, status);
    if (status.keyOnDisk) store.set(linearKeyOnDiskAtom, status.keyOnDisk);
  } catch (err) {
    console.warn("[linear] sync status read failed:", err);
  }
  await refreshLinearMirror(store);

  return unlisteners;
}
