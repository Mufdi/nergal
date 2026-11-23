import { useRef, useEffect } from "react";
import { TriangleAlert } from "lucide-react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  workspacesAtom,
  activeSessionIdAtom,
  activeSessionAtom,
  activeWorkspaceAtom,
  sessionLaunchModeAtom,
  freshSessionsAtom,
} from "@/stores/workspace";
import { appStore } from "@/stores/jotaiStore";
import { readingTerminalAtom } from "@/stores/readingMode";
import { focusZoneAtom } from "@/stores/shortcuts";
import { confirmAndDeleteSession, deleteSessionWithGraceAction } from "@/stores/pendingDeletes";
import { launchAndConsume } from "@/lib/launchMode";
import { posixQuote } from "@/lib/shellQuote";
import { Button } from "@/components/ui/button";
import { spawnEnvShells } from "@/stores/quake";
import * as terminalService from "./terminalService";
import { NergalMark } from "@/components/layout/NergalLogo";

/// Thin React wrapper over the canvas terminal service. The service owns all
/// DOM containers, PTY subscriptions, and render state outside React's
/// lifecycle — this component only provides the host div and signals
/// session visibility + focus intent via atoms.
export function TerminalManager() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const activeSession = useAtomValue(activeSessionAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const launchModes = useAtomValue(sessionLaunchModeAtom);
  const freshSessions = useAtomValue(freshSessionsAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const setFreshSessions = useSetAtom(freshSessionsAtom);
  const deleteWithGrace = useSetAtom(deleteSessionWithGraceAction);
  const hostRef = useRef<HTMLDivElement>(null);
  const deleteButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    terminalService.setHost(hostRef.current);
    return () => terminalService.setHost(null);
  }, []);

  useEffect(() => {
    // Last tab soft-closed → show the empty state now, not after the undo TTL.
    if (!activeSessionId) {
      terminalService.hideAll();
      return;
    }
    if (!activeSession || !activeWorkspace) return;
    if (activeSession.status === "completed") return;

    if (activeSession.worktree_pending_delete) {
      terminalService.hideAll();
      return;
    }

    const { mode, nextFresh } = launchAndConsume(activeSessionId, launchModes, freshSessions);
    if (nextFresh.size !== freshSessions.size) setFreshSessions(nextFresh);

    const cwd = activeSession.worktree_path ?? activeWorkspace.repo_path;
    void terminalService.show(activeSessionId, cwd, mode);

    // Re-open after restart: seed the quake tabs from the persisted defs,
    // pre-filled (the quake spawns lazily on view; Enter re-runs). Fresh
    // sessions were already seeded auto-run by the creation flow, which
    // makes this a no-op for them.
    if (activeSession.env_shells?.length) {
      spawnEnvShells(activeSessionId, cwd, activeWorkspace.repo_path, activeSession.env_shells, false);
    }
  }, [activeSessionId, activeSession, activeWorkspace, launchModes, freshSessions, setFreshSessions]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => terminalService.fitActive());
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  useEffect(
    () => appStore.sub(readingTerminalAtom, () => requestAnimationFrame(() => terminalService.fitActive())),
    [],
  );

  // When the global focus zone flips to "terminal" (e.g. Ctrl+Ñ toggle,
  // pane cycle), snap focus back into the terminal's input surface so the
  // user doesn't need to click to start typing.
  useEffect(() => {
    if (focusZone === "terminal") {
      terminalService.focusActive();
    }
  }, [focusZone]);

  const pendingSession = activeSession?.worktree_pending_delete ? activeSession : null;

  const pendingSessionId = pendingSession?.id ?? null;
  useEffect(() => {
    if (pendingSessionId !== null && focusZone === "terminal") {
      deleteButtonRef.current?.focus();
    }
  }, [pendingSessionId, focusZone]);

  function finishDelete() {
    if (!pendingSession) return Promise.resolve();
    return confirmAndDeleteSession(pendingSession, deleteWithGrace);
  }

  const hasAnySessions = workspaces.some((ws) => ws.sessions.length > 0);

  return (
    <div
      ref={hostRef}
      data-focus-zone="terminal"
      className="relative h-full w-full"
      onMouseDown={() => {
        setFocusZone("terminal");
        // Setting the atom is a no-op when the zone was already "terminal",
        // so the focus-restoring effect wouldn't re-fire. Force a focus pass
        // here so a click on a non-canvas region (padding/gap/empty state)
        // still drags the hidden textarea back as the active element.
        terminalService.focusActive();
      }}
    >
      {pendingSession && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 px-8 text-center">
          <TriangleAlert className="size-5 text-yellow-500" />
          <p className="max-w-md text-[12px] text-terminal-foreground/70">
            This worktree was partly deleted. Press Delete to finish. If it fails again, run:
          </p>
          <code className="max-w-md select-text break-all rounded bg-muted/50 px-2 py-1 text-[11px]">
            sudo rm -rf {posixQuote(pendingSession.worktree_path ?? "")}
          </code>
          <p className="text-[12px] text-terminal-foreground/70">Then press Delete.</p>
          <Button
            ref={deleteButtonRef}
            type="button"
            variant="destructive"
            size="sm"
            onClick={() => void finishDelete()}
            className="focus:border-destructive/40 focus:ring-2 focus:ring-inset focus:ring-destructive/40"
          >
            Delete
          </Button>
        </div>
      )}
      {(!hasAnySessions || !activeSessionId) && (
        <div className="flex h-full flex-col items-center justify-center gap-6 select-none">
          <NergalMark className="h-[min(70%,420px)] w-auto" />
          <span className="text-[11px] text-terminal-foreground/40">Select or create a session</span>
        </div>
      )}
    </div>
  );
}
