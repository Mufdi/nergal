import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";
import { toastsAtom } from "./toast";
import {
  activeSessionIdAtom,
  activeWorkspaceAtom,
  expandedWorkspaceIdsAtom,
  freshSessionsAtom,
  workspacesAtom,
  type Session,
} from "./workspace";

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
  name: string;
  list_id: string;
  list_name: string;
  space_id: string;
  parent_id: string | null;
  status_name: string | null;
  status_color: string | null;
  priority: string | null;
  assignees: ClickUpAssignee[];
  tags: ClickUpTag[];
  due_date: number | null;
  start_date: number | null;
  date_updated: number | null;
  url: string | null;
  archived: boolean;
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

// ── UI prefs ──
// Module-level atoms: same persistence tier as gitChipModeAtom /
// specSubTabMapAtom — survive panel close/reopen and session switches
// within the app run.

export type ClickUpGroupBy = "status" | "list" | "assignee";
export const GROUP_BY_ORDER: ClickUpGroupBy[] = ["status", "list", "assignee"];

/// null = "Todos" (all Spaces).
export const clickupSpaceFilterAtom = atom<string | null>(null);
export const clickupGroupByAtom = atom<ClickUpGroupBy>("status");
export const clickupAssignedToMeAtom = atom(false);
export const clickupShowClosedAtom = atom(false);

/// Ephemeral result of the on-demand show-closed fetch — merged client-side
/// for display, never part of the mirror read.
export const clickupClosedTasksAtom = atom<ClickUpTask[]>([]);

/// Task currently open in the floating detail module (null = closed).
export const clickupDetailTaskIdAtom = atom<string | null>(null);

/// `token_on_disk` disclosure from the last set_token of this app run; the
/// backend only reports it at store time, so it resets on restart.
export const clickupTokenOnDiskAtom = atom(false);

// ── Session binding + task-to-agent verbs (clickup-task-integration) ──

export interface ClickUpComposeConfirm {
  markdown: string;
  /// True only when the session's Running edge was observed at runtime —
  /// never derived from static hook config alone.
  guard_active: boolean;
  guard_hint: "active" | "restart_session" | "run_setup";
}

export interface ClickUpSendOutcome {
  status: "delivered" | "queued";
  replaced: boolean;
}

interface SendQueuedPayload {
  session_id: string;
  task_id: string;
  replaced: boolean;
}

interface SendDeliveredPayload {
  session_id: string;
  task_id: string;
  /// immediate = pasted + submitted; deferred/forced = pasted WITHOUT submit
  /// (the user reviews and presses Enter).
  mode: "immediate" | "deferred" | "forced";
}

interface SendDroppedPayload {
  session_id: string;
  task_id: string | null;
  reason: string;
}

/// session_id → active task id. `null` = explicitly unbound this run; an
/// absent key falls back to the Session row (same seeding pattern as
/// `pinnedNotesMapAtom`).
export const clickupBindingMapAtom = atom<Record<string, string | null>>({});

/// session_id → pinned task ids (command results are authoritative; absent
/// key falls back to the Session row).
export const clickupPinsMapAtom = atom<Record<string, string[]>>({});

/// session_id → queued send. Set on `clickup:send-queued`, cleared on
/// delivered/dropped — the backend guarantees every queued send resolves
/// through one of those events, so the UI never shows a stale "queued".
export const clickupQueuedSendMapAtom = atom<Record<string, { taskId: string }>>({});

/// Pending send-as-prompt confirmation (Decision 6: untrusted content is
/// never auto-submitted without the user seeing it). null = dialog closed.
export const clickupSendConfirmAtom = atom<{ sessionId: string; taskId: string } | null>(null);

/// Pending rebind confirmation: binding over an existing active task.
export const clickupRebindConfirmAtom = atom<{
  sessionId: string;
  taskId: string;
  currentTaskId: string;
} | null>(null);

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

function taskLabel(store: Store, taskId: string | null): string {
  if (!taskId) return "task";
  return store.get(clickupTasksAtom).find((t) => t.id === taskId)?.name ?? taskId;
}

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
}

/// Action labels advertise the contextual key (tooltip-as-source-of-truth).
export const CLICKUP_ACTION_LABELS = {
  send: "Send as prompt (S)",
  spawn: "Spawn worktree session (W)",
  pin: "Attach as context (P)",
  unpin: `Unpin (P) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
  bind: "Bind as active task (B)",
  unbind: `Unbind (B) — ${FUTURE_SPAWNS_HINT.toLowerCase()}`,
} as const;

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
  async (_get, set, args: { sessionId: string; taskId: string }) => {
    try {
      await invoke("clickup_bind_task", args);
      set(clickupBindingMapAtom, (prev) => ({ ...prev, [args.sessionId]: args.taskId }));
      set(toastsAtom, {
        message: "Bound as active task",
        description:
          "Write-back target for this session (shown in the tab chip); injected at the next spawn/resume.",
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
    set(clickupRebindConfirmAtom, { sessionId, taskId, currentTaskId: current });
    return;
  }
  await set(performBindTaskAction, { sessionId, taskId });
});

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
    set(toastsAtom, {
      message: `Worktree session: ${session.name}`,
      description: "Task bound and queued as the initial prompt.",
      type: "success",
    });
  } catch (err) {
    set(toastsAtom, { message: "Spawn worktree failed", description: String(err), type: "error" });
  }
});

export const cancelQueuedSendAction = atom(null, async (_get, set, sessionId: string) => {
  try {
    const existed = await invoke<boolean>("clickup_cancel_queued_send", { sessionId });
    // Outcome toast + map cleanup ride the clickup:send-dropped event; only
    // an already-empty queue (raced drain) needs the defensive clear.
    if (!existed) {
      set(clickupQueuedSendMapAtom, (prev) => {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      });
    }
  } catch (err) {
    set(toastsAtom, { message: "Cancel failed", description: String(err), type: "error" });
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

export const forceDeliverQueuedSendAction = atom(null, async (_get, set, sessionId: string) => {
  try {
    const existed = await invoke<boolean>("clickup_force_deliver_queued_send", { sessionId });
    if (!existed) {
      set(clickupQueuedSendMapAtom, (prev) => {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      });
    }
  } catch (err) {
    set(toastsAtom, { message: "Deliver now failed", description: String(err), type: "error" });
  }
});

// ── Refresh + bootstrap ──

export async function refreshClickUpMirror(store: Store): Promise<void> {
  try {
    const [tasks, spaces] = await Promise.all([
      invoke<ClickUpTask[]>("clickup_read_tasks", {}),
      invoke<ClickUpSpace[]>("clickup_read_spaces"),
    ]);
    store.set(clickupTasksAtom, tasks);
    store.set(clickupSpacesAtom, spaces);
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

  unlisteners.push(
    await listen<SendQueuedPayload>("clickup:send-queued", (p) => {
      store.set(clickupQueuedSendMapAtom, (prev) => ({
        ...prev,
        [p.session_id]: { taskId: p.task_id },
      }));
      store.set(toastsAtom, {
        message: "Send queued",
        description: `${taskLabel(store, p.task_id)} — delivers when the agent finishes the current turn.${
          p.replaced ? " Replaced the previously queued send." : ""
        }`,
        type: "info",
      });
    }),
  );

  unlisteners.push(
    await listen<SendDeliveredPayload>("clickup:send-delivered", (p) => {
      store.set(clickupQueuedSendMapAtom, (prev) => {
        const next = { ...prev };
        delete next[p.session_id];
        return next;
      });
      store.set(
        toastsAtom,
        p.mode === "immediate"
          ? {
              message: "Task sent as prompt",
              description: taskLabel(store, p.task_id),
              type: "success",
            }
          : {
              message: "Task pasted into the session",
              description: `${taskLabel(store, p.task_id)} — review and press Enter to submit.`,
              type: "success",
            },
      );
    }),
  );

  unlisteners.push(
    await listen<SendDroppedPayload>("clickup:send-dropped", (p) => {
      store.set(clickupQueuedSendMapAtom, (prev) => {
        const next = { ...prev };
        delete next[p.session_id];
        return next;
      });
      store.set(
        toastsAtom,
        p.reason === "cancelled"
          ? {
              message: "Queued send cancelled",
              description: taskLabel(store, p.task_id),
              type: "info",
            }
          : {
              message: "Send dropped",
              description: `${taskLabel(store, p.task_id)} — ${p.reason}`,
              type: "error",
            },
      );
    }),
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
