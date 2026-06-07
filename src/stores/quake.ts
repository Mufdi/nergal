import { atom } from "jotai";
import { invoke, listen, generateId } from "@/lib/tauri";
import { appStore } from "./jotaiStore";
import { activeSessionIdAtom } from "./workspace";
import * as terminalService from "@/components/terminal/terminalService";

export interface QuakeShell {
  shellId: string;
  label: string;
  /// Last command associated with the shell: the env-shell def at creation,
  /// then updated to whatever the user last submitted. Null until a command
  /// runs in an ad-hoc shell.
  command: string | null;
}

/// Visibility is per-session, like the right panel's collapsed map: switching
/// to a session where the quake was closed keeps it closed.
export const quakeOpenMapAtom = atom<Record<string, boolean>>({});
export const quakeHeightAtom = atom(300);
export const quakeShellsAtom = atom<Record<string, QuakeShell[]>>({});
export const activeQuakeShellAtom = atom<Record<string, string | null>>({});

export function newShellId(): string {
  return generateId("shell");
}

/// Mirror the session's live tab set to the DB so re-open recreates it
/// pre-filled ("remember the set" — including commands run in ad-hoc tabs).
function persistShells(sessionId: string): void {
  const shells = appStore.get(quakeShellsAtom)[sessionId] ?? [];
  invoke("update_session_env_shells", {
    sessionId,
    envShells: shells.map((sh) => ({ label: sh.label, command: sh.command ?? "" })),
  }).catch(() => {});
}

export function addAdHocShell(sessionId: string): void {
  const s = appStore;
  const shellId = newShellId();
  const count = (s.get(quakeShellsAtom)[sessionId] ?? []).length;
  s.set(quakeShellsAtom, (prev) => ({
    ...prev,
    [sessionId]: [
      ...(prev[sessionId] ?? []),
      { shellId, label: `shell ${count + 1}`, command: null },
    ],
  }));
  s.set(activeQuakeShellAtom, (prev) => ({ ...prev, [sessionId]: shellId }));
  persistShells(sessionId);
}

export function removeShell(sessionId: string, shellId: string, killPty: boolean): void {
  const s = appStore;
  if (killPty) {
    invoke("kill_aux_shell", { sessionId, shellId }).catch(() => {});
  }
  terminalService.dropShellEntry(sessionId, shellId);
  const remaining = (s.get(quakeShellsAtom)[sessionId] ?? []).filter(
    (sh) => sh.shellId !== shellId,
  );
  s.set(quakeShellsAtom, (prev) => ({ ...prev, [sessionId]: remaining }));
  s.set(activeQuakeShellAtom, (prev) => {
    if (prev[sessionId] !== shellId) return prev;
    return { ...prev, [sessionId]: remaining[0]?.shellId ?? null };
  });
  persistShells(sessionId);
}

/// Ctrl+Tab / Ctrl+Shift+Tab with focus in the quake zone.
export function cycleQuakeShell(direction: 1 | -1): void {
  const s = appStore;
  const sessionId = s.get(activeSessionIdAtom);
  if (!sessionId) return;
  const shells = s.get(quakeShellsAtom)[sessionId] ?? [];
  if (shells.length < 2) return;
  const activeId = s.get(activeQuakeShellAtom)[sessionId] ?? shells[0].shellId;
  const idx = shells.findIndex((sh) => sh.shellId === activeId);
  const next = shells[(idx + direction + shells.length) % shells.length];
  s.set(activeQuakeShellAtom, (prev) => ({ ...prev, [sessionId]: next.shellId }));
}

/// Ctrl+W with focus in the quake zone.
export function closeActiveQuakeShell(): void {
  const s = appStore;
  const sessionId = s.get(activeSessionIdAtom);
  if (!sessionId) return;
  const shells = s.get(quakeShellsAtom)[sessionId] ?? [];
  const activeId = s.get(activeQuakeShellAtom)[sessionId] ?? shells[0]?.shellId ?? null;
  if (!activeId) return;
  removeShell(sessionId, activeId, true);
}

/// Seed a session's quake tabs from its environment-shell defs. With
/// `autorun` (session creation) every command spawns and runs immediately,
/// headless — the quake doesn't need to be open. Without it (re-open after
/// restart) only the tabs are seeded; the quake component spawns lazily with
/// the command pre-filled.
export function spawnEnvShells(
  sessionId: string,
  cwd: string,
  defs: Array<{ label: string; command: string }>,
  autorun: boolean,
): void {
  if (defs.length === 0) return;
  const s = appStore;
  // Presence-of-key check, not length: an empty array is the tombstone left
  // when the user closed every tab — re-seeding would resurrect them on the
  // next session-object churn.
  if (sessionId in s.get(quakeShellsAtom)) return;
  const shells: QuakeShell[] = defs.map((d, i) => ({
    shellId: `env-${i}`,
    label: d.label.trim() || `shell ${i + 1}`,
    command: d.command.trim() || null,
  }));
  s.set(quakeShellsAtom, (prev) => ({ ...prev, [sessionId]: shells }));
  s.set(activeQuakeShellAtom, (prev) => ({
    ...prev,
    [sessionId]: shells[0].shellId,
  }));
  if (!autorun) return;
  for (const sh of shells) {
    if (!sh.command) continue;
    // 200x50 placeholder grid — the quake host doesn't exist yet; fit()
    // resizes on first view.
    invoke("spawn_aux_shell", {
      sessionId,
      shellId: sh.shellId,
      cwd,
      cols: 200,
      rows: 50,
      command: sh.command,
      autorun: true,
    }).catch(() => {});
  }
}

function splitTermId(termId: string): { sessionId: string; shellId: string } | null {
  const sep = termId.indexOf("::");
  if (sep < 0) return null;
  return { sessionId: termId.slice(0, sep), shellId: termId.slice(sep + 2) };
}

/// PTY EOF (user typed `exit`, process died). The backend only signals;
/// frontend retires the tab and asks it to clean its maps so a later shell
/// with the same id can respawn.
void listen<string>("shell:exited", (termId) => {
  const ids = splitTermId(termId);
  if (ids) removeShell(ids.sessionId, ids.shellId, true);
});

/// A command was submitted in an aux shell — remember it on the tab def so
/// re-open pre-fills it.
void listen<{ termId: string; command: string }>("shell:command", ({ termId, command }) => {
  const ids = splitTermId(termId);
  if (!ids) return;
  const s = appStore;
  const shells = s.get(quakeShellsAtom)[ids.sessionId];
  if (!shells?.some((sh) => sh.shellId === ids.shellId)) return;
  s.set(quakeShellsAtom, (prev) => ({
    ...prev,
    [ids.sessionId]: (prev[ids.sessionId] ?? []).map((sh) =>
      sh.shellId === ids.shellId ? { ...sh, command } : sh,
    ),
  }));
  persistShells(ids.sessionId);
});
