import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";

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

  try {
    const status = await invoke<ClickUpSyncStatus>("clickup_sync_status");
    store.set(clickupSyncStatusAtom, status);
  } catch (err) {
    console.warn("[clickup] sync status read failed:", err);
  }
  await refreshClickUpMirror(store);

  return unlisteners;
}
