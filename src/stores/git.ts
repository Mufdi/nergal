import { atom } from "jotai";
import { activeSessionIdAtom } from "./workspace";
import { configAtom } from "./config";
import { invoke } from "@/lib/tauri";
import type { Config } from "@/lib/types";

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

/// Tracks which sessions had auto-merge enabled when Ship was invoked.
/// When the same session later surfaces conflicted files, the UI knows the
/// conflict came from the auto-merge path and can offer the closed-loop
/// handoff (open Conflicts panel + pre-fill Ask-Claude prompt).
export const sessionsAutoMergedAtom = atom<Set<string>>(new Set<string>());

/// Read+write atom for the persisted "auto-merge by default" preference.
/// Reads from configAtom; writes update the atom AND persist to disk via
/// `save_config`. Backend `Config::default()` ships this as `true` so first
/// run starts with auto-merge on.
export const autoMergeDefaultAtom = atom(
  (get) => get(configAtom).git_auto_merge_default,
  (get, set, next: boolean) => {
    const current = get(configAtom);
    if (current.git_auto_merge_default === next) return;
    const updated: Config = { ...current, git_auto_merge_default: next };
    set(configAtom, updated);
    void invoke("save_config", { config: updated }).catch((err: unknown) => {
      console.error("save_config(auto_merge_default) failed", err);
    });
  },
);
