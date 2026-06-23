import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";
import { confirm as swalConfirm } from "@/lib/swal";
import { toastsAtom } from "./toast";
import { openTabAction } from "./rightPanel";
import { focusZoneAtom } from "./shortcuts";
import {
  activeSessionIdAtom,
  activeWorkspaceAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
} from "./workspace";

// ── Writeback: optimistic overlay (task 2.1) ──

/// Per-field key for the overlay atom.
export type OverlayKey = `${string}:${string}`; // `${taskId}:${field}`

/// A pending edit that has been sent to the API but not yet ack'd by a
/// mirror reconcile. The overlay is volatile — it is never written to SQLite.
export interface OverlayEntry {
  value: string | null; // null = clear (due date cleared)
}

/// In-memory optimistic overlay keyed by `${taskId}:${field}`. Cleared on
/// API success (mirror reconcile will carry the truth); reverted on failure.
export const clickupOverlayAtom = atom<Record<OverlayKey, OverlayEntry>>({});

/// Merge the live overlay into a task view for display (Decision 1).
/// Returns a shallow-merged copy; the durable mirror is never mutated.
export function applyOverlay<T extends { id: string }>(
  task: T,
  overlay: Record<OverlayKey, OverlayEntry>,
  mergeFields: Partial<Record<string, (entry: OverlayEntry, t: T) => Partial<T>>>,
): T {
  let merged = task;
  for (const [field, merger] of Object.entries(mergeFields)) {
    if (!merger) continue;
    const key: OverlayKey = `${task.id}:${field}`;
    const entry = overlay[key];
    if (entry !== undefined) {
      merged = { ...merged, ...merger(entry, merged) };
    }
  }
  return merged;
}

/// Set an overlay entry for `taskId:field`.
export function setOverlayEntry(
  set: (updater: (prev: Record<OverlayKey, OverlayEntry>) => Record<OverlayKey, OverlayEntry>) => void,
  taskId: string,
  field: string,
  value: string | null,
) {
  const key: OverlayKey = `${taskId}:${field}`;
  set((prev) => ({ ...prev, [key]: { value } }));
}

/// Clear an overlay entry (on API success or failure).
export function clearOverlayEntry(
  set: (updater: (prev: Record<OverlayKey, OverlayEntry>) => Record<OverlayKey, OverlayEntry>) => void,
  taskId: string,
  field: string,
) {
  const key: OverlayKey = `${taskId}:${field}`;
  set((prev) => {
    const next = { ...prev };
    delete next[key];
    return next;
  });
}

// ── Closure offer atom (task 5.2) ──

export interface ClickUpClosureOffer {
  taskId: string;
  sessionId: string;
  prUrl?: string;
}

/// Non-null when the closure prompt is open. Raised by ship-success (prUrl
/// set) or by the manual "Close out task" verb (no prUrl).
export const clickupClosureOfferAtom = atom<ClickUpClosureOffer | null>(null);

// ── Shared list-status shape (matches mirror.rs StatusView serde output) ──

export interface ClickUpListStatus {
  name: string;
  color: string | null;
  orderindex: number | null;
  status_type: string | null;
}

/// list_id → its ordered workflow (mirror-cached). Feeds the panel's status
/// glyphs: the proportional "in progress" pie fill needs the full ordered set
/// to know a status's position. Lazily filled — bulk mirror read on mount, then
/// a background live-resolve for any visible list not yet cached.
export const clickupListStatusesAtom = atom<Record<string, ClickUpListStatus[]>>({});

/// Pie fill for the "in progress" glyph: the status's position in its list's
/// ordered workflow (statuses arrive ordered by orderindex). An unknown status
/// or a list with <2 statuses falls back to a half-pie until the set resolves.
export function statusFraction(statuses: ClickUpListStatus[] | undefined, name: string | null): number {
  if (!name || !statuses || statuses.length < 2) return 0.5;
  const idx = statuses.findIndex((s) => s.name === name);
  if (idx < 0) return 0.5;
  return idx / (statuses.length - 1);
}

type Store = ReturnType<typeof getDefaultStore>;

// ── Backend view shapes (snake_case mirrors the Rust serde output) ──

export interface ClickUpAssignee {
  id: number | null;
  username: string | null;
  color: string | null;
  initials: string | null;
}

export interface ClickUpTag {
  name: string;
  tag_fg: string | null;
  tag_bg: string | null;
}

export interface ClickUpTask {
  id: string;
  custom_id: string | null;
  name: string;
  list_id: string;
  list_name: string;
  space_id: string;
  parent_id: string | null;
  status_name: string | null;
  status_color: string | null;
  status_type: string | null;
  priority: string | null;
  assignees: ClickUpAssignee[];
  tags: ClickUpTag[];
  due_date: number | null;
  start_date: number | null;
  date_created: number | null;
  date_updated: number | null;
  url: string | null;
  archived: boolean;
  has_description: boolean;
  subtask_count: number;
  checklist_count: number;
  attachment_count: number;
  stale: boolean;
}

export interface ClickUpSpace {
  id: string;
  name: string;
}

export interface ClickUpTeam {
  id: string;
  name: string;
}

export interface ClickUpSyncStatus {
  state: "idle" | "no_token" | "needs_team" | "syncing" | "ok" | "error";
  teams: ClickUpTeam[];
  team_id: string | null;
  user_id: number | null;
  last_sync: number | null;
  baseline_done: boolean;
  error: string | null;
}

export interface ClickUpComment {
  id: string;
  user: ClickUpAssignee | null;
  text: string | null;
  date: number | null;
  resolved: boolean;
  reply_count: number;
}

export interface ClickUpChecklistItem {
  id: string;
  name: string | null;
  resolved: boolean;
  orderindex: number | null;
}

export interface ClickUpChecklist {
  id: string;
  name: string | null;
  orderindex: number | null;
  items: ClickUpChecklistItem[];
}

export interface ClickUpAttachment {
  id: string;
  title: string | null;
  url: string | null;
  mimetype: string | null;
  size: number | null;
  thumbnail_url: string | null;
}

export interface ClickUpCustomValue {
  field_id: string;
  name: string;
  field_type: string;
  type_config_json: string | null;
  value_json: string | null;
}

export interface ClickUpTaskDetailData {
  task: ClickUpTask | null;
  description: string | null;
  comments: ClickUpComment[];
  checklists: ClickUpChecklist[];
  attachments: ClickUpAttachment[];
  custom_values: ClickUpCustomValue[];
}

// ── Data atoms (fed by clickup:changed / clickup:sync-status) ──

export const clickupSyncStatusAtom = atom<ClickUpSyncStatus | null>(null);

/// Gates the TopBar entry + shortcut: entry point exists only with a token.
/// "idle" is the transient pre-resolution state at startup — treated as
/// not-configured so the icon doesn't flash for tokenless installs.
export const clickupConfiguredAtom = atom((get) => {
  const status = get(clickupSyncStatusAtom);
  return status !== null && status.state !== "no_token" && status.state !== "idle";
});

/// Open (non-tombstoned) mirror rows. Closed/ephemeral rows live separately
/// in `clickupClosedTasksAtom` so toggling show-closed never pollutes this.
export const clickupTasksAtom = atom<ClickUpTask[]>([]);
export const clickupSpacesAtom = atom<ClickUpSpace[]>([]);

/// Task ids closed out from a session (worked-and-done). Local marker, fully
/// separate from the ClickUp status — drives the panel/detail badge.
export const clickupClosedOutAtom = atom<string[]>([]);

// ── UI prefs ──
// Module-level atoms: same persistence tier as gitChipModeAtom /
// specSubTabMapAtom — survive panel close/reopen and session switches
// within the app run.

export type ClickUpGroupBy = "status" | "list" | "assignee";
export const GROUP_BY_ORDER: ClickUpGroupBy[] = ["status", "list", "assignee"];

/// Sort applied to tasks within each group (and to nested subtask siblings).
export type ClickUpSortField = "updated" | "created" | "priority" | "due";
export interface ClickUpSort {
  field: ClickUpSortField;
  dir: "asc" | "desc";
}
export const clickupSortAtom = atom<ClickUpSort>({ field: "updated", dir: "desc" });

/// null = "Todos" (all Spaces).
export const clickupSpaceFilterAtom = atom<string | null>(null);
export const clickupGroupByAtom = atom<ClickUpGroupBy>("status");
/// Default true: assigned-to-me + status grouping IS the "My tasks" chip,
/// the panel's default view.
export const clickupAssignedToMeAtom = atom(true);
export const clickupShowClosedAtom = atom(false);

/// Ephemeral result of the on-demand show-closed fetch — merged client-side
/// for display, never part of the mirror read.
export const clickupClosedTasksAtom = atom<ClickUpTask[]>([]);

/// Task currently open in the floating detail module (null = closed).
export const clickupDetailTaskIdAtom = atom<string | null>(null);

/// One-shot flag: when a worktree spawn closes the floating detail, its
/// close-effect must NOT restore focus to the panel — focus belongs to the new
/// session's terminal. Mirrors Linear's `_suppressDetailCloseFocus`.
let _suppressDetailCloseFocus = false;
export function suppressClickUpDetailCloseFocus(): void {
  _suppressDetailCloseFocus = true;
}
export function consumeClickUpDetailCloseFocusSuppress(): boolean {
  const v = _suppressDetailCloseFocus;
  _suppressDetailCloseFocus = false;
  return v;
}

/// `token_on_disk` disclosure from the last set_token of this app run; the
/// backend only reports it at store time, so it resets on restart.
export const clickupTokenOnDiskAtom = atom(false);

// ── Session binding + task-to-agent verbs (clickup-task-integration) ──

/// session_id → active task id. `null` = explicitly unbound this run; an
/// absent key falls back to the Session row (same seeding pattern as
/// `pinnedNotesMapAtom`).
export const clickupBindingMapAtom = atom<Record<string, string | null>>({});

/// session_id → pinned task ids (command results are authoritative; absent
/// key falls back to the Session row).
export const clickupPinsMapAtom = atom<Record<string, string[]>>({});

/// Pending send-as-prompt confirmation (Decision 6: the send auto-submits a
/// turn, so the user reviews the composed block first). null = dialog closed.
export const clickupSendConfirmAtom = atom<{ sessionId: string; taskId: string } | null>(null);

/// Shared binding resolution for surfaces that render per-session rows
/// (session tabs): runtime map wins, Session row seeds.
export function resolveActiveClickUpTask(
  map: Record<string, string | null>,
  session: Pick<Session, "id" | "active_clickup_task_id">,
): string | null {
  const runtime = map[session.id];
  return runtime !== undefined ? runtime : session.active_clickup_task_id ?? null;
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
/// findSession so ShipDialog.tsx can resolve the shipped session's bound task
/// without reading the active session (which may differ from the shipped one).
export function resolveActiveClickUpTaskById(
  workspaces: { sessions: Session[] }[],
  bindingMap: Record<string, string | null>,
  sessionId: string,
): string | null {
  const runtime = bindingMap[sessionId];
  if (runtime !== undefined) return runtime;
  return findSession(workspaces, sessionId)?.active_clickup_task_id ?? null;
}

export const activeSessionClickUpTaskAtom = atom<string | null>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return null;
  const runtime = get(clickupBindingMapAtom)[id];
  if (runtime !== undefined) return runtime;
  return findSession(get(workspacesAtom), id)?.active_clickup_task_id ?? null;
});

export const activeSessionClickUpPinsAtom = atom<string[]>((get) => {
  const id = get(activeSessionIdAtom);
  if (!id) return [];
  const runtime = get(clickupPinsMapAtom)[id];
  if (runtime !== undefined) return runtime;
  return findSession(get(workspacesAtom), id)?.pinned_clickup_task_ids ?? [];
});

/// Decision 7: unbind/unpin affect future spawns/resumes only — context
/// already in a running agent's window is not retracted.
export const FUTURE_SPAWNS_HINT =
  "Affects future spawns/resumes — the live window keeps its context.";

/// Task verbs shared by the panel rows, the floating-detail toolbar and the
/// contextual keys (S/W/P/B inside the clickup zone).
export interface ClickUpTaskActions {
  send: (taskId: string) => void;
  spawn: (taskId: string) => void;
  togglePin: (taskId: string) => void;
  toggleBind: (taskId: string) => void;
  reinject: (taskId: string) => void;
  closeOut: (taskId: string) => void;
  openInClickup: (taskId: string) => void;
  openTab: (taskId: string) => void;
  copyId: (displayId: string) => void;
}

/// Action labels advertise the contextual key (tooltip-as-source-of-truth).
export const CLICKUP_ACTION_LABELS = {
  send: "Send as prompt (S)",
  spawn: "Spawn worktree session (W)",
  pin: "Attach as context (P)",
  unpin: `Unpin (P) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
  bind: "Bind as active task (B)",
  unbind: `Unbind (B) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
  reinject: "Re-inject into the live session (R)",
  closeOut: "Close out task (C) — mark done & unbind",
  openInClickup: "Open in ClickUp (O)",
  openAsTab: "Open as tab (T)",
} as const;

/// Copy a task identifier (custom id when present, else the internal id) to the
/// OS clipboard + toast. Uses the robust spawn_blocking writer (the plugin's
/// Wayland backend stalls async tasks — see terminalService).
export const copyTaskIdAction = atom(null, (_get, set, taskId: string) => {
  void invoke("terminal_clipboard_write", { text: taskId })
    .then(() =>
      set(toastsAtom, { message: "Task ID copied", description: taskId, type: "success" }),
    )
    .catch((err) =>
      set(toastsAtom, { message: "Copy failed", description: String(err), type: "error" }),
    );
});

/// Open a task as a full document tab (single-column detail). Non-singleton:
/// each task gets its own tab keyed by id; a second open of the same task
/// reactivates the existing tab instead of duplicating it. The tab label is the
/// task name (the TabBar truncates it visually).
export const openClickUpTaskTabAction = atom(null, (get, set, taskId: string) => {
  if (!get(activeSessionIdAtom)) {
    set(toastsAtom, {
      message: "Open as tab",
      description: "Tabs are session-scoped — select a session first.",
      type: "info",
    });
    return;
  }
  const task =
    get(clickupTasksAtom).find((t) => t.id === taskId) ??
    get(clickupClosedTasksAtom).find((t) => t.id === taskId);
  set(openTabAction, {
    tab: {
      id: `clickup-task:${taskId}`,
      type: "clickup-task",
      label: task?.name ?? "Task",
      data: { taskId },
    },
    activate: true,
  });
});

export const requestSendTaskAction = atom(null, (get, set, taskId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Send as prompt",
      description: "No active session to send to.",
      type: "info",
    });
    return;
  }
  set(clickupSendConfirmAtom, { sessionId, taskId });
});

export const togglePinTaskAction = atom(null, async (get, set, taskId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Attach as context",
      description: "No active session to attach to.",
      type: "info",
    });
    return;
  }
  const pinned = get(activeSessionClickUpPinsAtom).includes(taskId);
  try {
    const ids = await invoke<string[]>(pinned ? "clickup_unpin_task" : "clickup_pin_task", {
      sessionId,
      taskId,
    });
    set(clickupPinsMapAtom, (prev) => ({ ...prev, [sessionId]: ids }));
    if (!pinned) {
      // Deliver the task to the live agent now (mirrors pinNoteAtom); the
      // persisted pin still seeds the next spawn/resume. No-op when the
      // session has no live PTY.
      void invoke("clickup_reinject_task", { sessionId, taskId }).catch(() => {});
    }
    set(
      toastsAtom,
      pinned
        ? { message: "Task unpinned", description: FUTURE_SPAWNS_HINT, type: "info" }
        : {
            message: "Task attached as context",
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

export const performBindTaskAction = atom(
  null,
  async (get, set, args: { sessionId: string; taskId: string }) => {
    try {
      await invoke("clickup_bind_task", args);
      set(clickupBindingMapAtom, (prev) => ({ ...prev, [args.sessionId]: args.taskId }));
      if (get(clickupDetailTaskIdAtom) !== null) set(clickupDetailTaskIdAtom, null);
      // Deliver the brief to the live agent now, SUBMITTED as a turn (binding
      // is the deliberate "work on this" act — the agent must ingest it
      // immediately, not wait in the input box). Still seeds future
      // spawns/resumes. No-op without a live PTY.
      void invoke("clickup_reinject_task", { ...args, submit: true }).catch(() => {});
      set(toastsAtom, {
        message: "Bound as active task",
        description:
          "Write-back target for this session (shown in the tab chip) — brief submitted to the live session; also persists for future spawns/resumes.",
        type: "success",
      });
    } catch (err) {
      set(toastsAtom, { message: "Bind failed", description: String(err), type: "error" });
    }
  },
);

export const unbindTaskAction = atom(null, async (get, set) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return;
  try {
    await invoke("clickup_unbind_task", { sessionId });
    set(clickupBindingMapAtom, (prev) => ({ ...prev, [sessionId]: null }));
    set(toastsAtom, { message: "Task unbound", description: FUTURE_SPAWNS_HINT, type: "info" });
  } catch (err) {
    set(toastsAtom, { message: "Unbind failed", description: String(err), type: "error" });
  }
});

/// Bind toggle: same task → unbind; different active task → confirm the
/// replacement (Decision 2 rebind rule); no active task → bind directly.
/// The replacement confirm uses the project's swal pattern (same as the
/// Sidebar's Delete/Remove confirms), not a bespoke Dialog.
export const requestBindTaskAction = atom(null, async (get, set, taskId: string) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) {
    set(toastsAtom, {
      message: "Bind task",
      description: "No active session to bind to.",
      type: "info",
    });
    return;
  }
  const current = get(activeSessionClickUpTaskAtom);
  if (current === taskId) {
    await set(unbindTaskAction);
    return;
  }
  if (current) {
    const name = (id: string) =>
      escapeHtml(get(clickupTasksAtom).find((t) => t.id === id)?.name ?? id);
    const confirmed = await swalConfirm({
      title: "Replace active task?",
      body: `This session is bound to "${name(current)}". Binding "${name(taskId)}" replaces it as the write-back target. The replaced task stays in ClickUp.`,
      confirmLabel: "Replace",
      kind: "question",
    });
    if (!confirmed) return;
  }
  await set(performBindTaskAction, { sessionId, taskId });
});

/// ClickUp task names are multi-writer input and swal bodies render as HTML.
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export const spawnWorktreeWithTaskAction = atom(null, async (get, set, taskId: string) => {
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
    const session = await invoke<Session>("clickup_spawn_worktree_with_task", {
      workspaceId: workspace.id,
      taskId,
    });
    set(workspacesAtom, (prev) =>
      prev.map((w) => (w.id === workspace.id ? { ...w, sessions: [...w.sessions, session] } : w)),
    );
    set(freshSessionsAtom, (prev) => new Set([...prev, session.id]));
    set(expandedWorkspaceIdsAtom, (prev) => new Set([...(prev ?? []), workspace.id]));
    // Activation spawns the PTY, which consumes the backend-queued initial
    // prompt — same flow as deep-link session/new.
    set(activeSessionIdAtom, session.id);
    if (get(clickupDetailTaskIdAtom) !== null) {
      suppressClickUpDetailCloseFocus();
      set(clickupDetailTaskIdAtom, null);
      set(focusZoneAtom, "terminal");
    }
    set(toastsAtom, {
      message: `Worktree session: ${session.name}`,
      description: "Task bound and queued as the initial prompt.",
      type: "success",
    });
  } catch (err) {
    set(toastsAtom, { message: "Spawn worktree failed", description: String(err), type: "error" });
  }
});

/// Explicit refresh of a live session's task context (design "Stale injected
/// context" risk, mirroring the Obsidian hot-reload rule): recompose +
/// bracketed paste into the live PTY, never automatic, never submits.
export const reinjectTaskAction = atom(null, async (get, set, taskId: string) => {
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
    await invoke("clickup_reinject_task", { sessionId, taskId });
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

export async function refreshClickUpMirror(store: Store): Promise<void> {
  try {
    const [tasks, spaces, closedOut] = await Promise.all([
      invoke<ClickUpTask[]>("clickup_read_tasks", {}),
      invoke<ClickUpSpace[]>("clickup_read_spaces"),
      invoke<string[]>("clickup_read_closed_out"),
    ]);
    store.set(clickupTasksAtom, tasks);
    store.set(clickupSpacesAtom, spaces);
    store.set(clickupClosedOutAtom, closedOut);
  } catch (err) {
    console.warn("[clickup] mirror read failed:", err);
  }
}

export async function setupClickUpListeners(store: Store): Promise<UnlistenFn[]> {
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<ClickUpSyncStatus>("clickup:sync-status", (payload) => {
      store.set(clickupSyncStatusAtom, payload);
    }),
  );

  // Payload is null by contract — refetch through the read commands.
  unlisteners.push(
    await listen<null>("clickup:changed", () => {
      void refreshClickUpMirror(store);
    }),
  );

  // New tasks assigned to the token user (coalesced by the poller). In-app
  // toast in addition to the desktop notification — the window may be focused
  // (desktop notifications get suppressed) and the user asked to see new tasks.
  unlisteners.push(
    await listen<string[]>("clickup:assigned", (names) => {
      if (!Array.isArray(names) || names.length === 0) return;
      store.set(toastsAtom, {
        message: names.length === 1 ? "New ClickUp task assigned" : `${names.length} new ClickUp tasks assigned`,
        description: names.length === 1 ? names[0] : names.join(" · "),
        type: "info",
      });
    }),
  );

  // Scalar write-conflict: remote superseded a local edit while we held an
  // overlay entry (Decision 3 / task 6.3). Warn via toast; the overlay is
  // already cleared by the failed/reverted path — this is a separate
  // notification path from the poller.
  unlisteners.push(
    await listen<{ task_id: string; field: string; your_value: string; remote_value: string }>(
      "clickup:write-conflict",
      (payload) => {
        store.set(toastsAtom, {
          message: "ClickUp write conflict",
          description: `${payload.field}: your change was superseded by a remote edit (remote: "${payload.remote_value}")`,
          type: "info",
        });
      },
    ),
  );

  try {
    const status = await invoke<ClickUpSyncStatus>("clickup_sync_status");
    store.set(clickupSyncStatusAtom, status);
  } catch (err) {
    console.warn("[clickup] sync status read failed:", err);
  }
  await refreshClickUpMirror(store);

  return unlisteners;
}
