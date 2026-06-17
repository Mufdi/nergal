import { atom, type getDefaultStore } from "jotai";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { invoke, listen } from "@/lib/tauri";
import { toastsAtom } from "./toast";

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
  assigneeId?: string;
  assigneeName?: string;
  projectId?: string;
  projectName?: string;
  cycleId?: string;
  parentId?: string;
  updatedAt?: number;
  url?: string;
  stale: boolean;
  labels: { id: string; name: string; color?: string }[];
}

export interface TeamView {
  id: string;
  name: string;
  key: string;
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

export type LinearGroupBy = "state" | "project" | "assignee";
export const GROUP_BY_ORDER: LinearGroupBy[] = ["state", "project", "assignee"];

/// null = "Todos" (all teams).
export const linearTeamFilterAtom = atom<string | null>(null);
export const linearGroupByAtom = atom<LinearGroupBy>("state");
/// Default true: show only issues assigned to the viewer.
export const linearAssignedToMeAtom = atom(true);
/// When true, completed/canceled issues show (local filter, no fetch).
export const linearShowCompletedAtom = atom(false);

/// Issue currently open in the floating detail module (null = closed).
export const linearDetailIssueIdAtom = atom<string | null>(null);

// ── Refresh + bootstrap ──

export async function refreshLinearMirror(store: Store): Promise<void> {
  try {
    const [issues, teams] = await Promise.all([
      invoke<IssueView[]>("linear_read_issues", {}),
      invoke<TeamView[]>("linear_read_teams"),
    ]);
    store.set(linearIssuesAtom, issues);
    store.set(linearTeamsAtom, teams);
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
