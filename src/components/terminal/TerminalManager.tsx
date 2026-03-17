import { useRef, useEffect } from "react";
import { useAtomValue } from "jotai";
import { workspacesAtom, activeSessionIdAtom, activeSessionAtom, activeWorkspaceAtom, sessionLaunchModeAtom, freshSessionsAtom } from "@/stores/workspace";
import * as terminalService from "./terminalService";

/// Thin React wrapper. Provides a host div and signals the service "show session X".
/// Service owns all DOM containers and xterm instances — React never touches them.
export function TerminalManager() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const activeSession = useAtomValue(activeSessionAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const launchModes = useAtomValue(sessionLaunchModeAtom);
  const freshSessions = useAtomValue(freshSessionsAtom);
  const hostRef = useRef<HTMLDivElement>(null);

  // Register host element once
  useEffect(() => {
    terminalService.setHost(hostRef.current);
    return () => terminalService.setHost(null);
  }, []);

  // When active session changes → tell service to show it
  useEffect(() => {
    if (!activeSessionId || !activeSession || !activeWorkspace) return;
    if (activeSession.status === "completed") return;

    let mode: "new" | "continue" | "resume_pick";
    if (launchModes[activeSessionId]) {
      mode = launchModes[activeSessionId];
    } else if (freshSessions.has(activeSessionId)) {
      mode = "new";
    } else {
      mode = "continue";
    }

    const cwd = activeSession.worktree_path ?? activeWorkspace.repo_path;
    terminalService.show(activeSessionId, cwd, mode);
  }, [activeSessionId, activeSession, activeWorkspace, launchModes, freshSessions]);

  // ResizeObserver on host → fit active terminal
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => terminalService.fitActive());
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  const hasAnySessions = workspaces.some((ws) => ws.sessions.length > 0);

  return (
    <div ref={hostRef} className="relative h-full w-full">
      {(!hasAnySessions || !activeSessionId) && (
        <div className="flex h-full items-center justify-center">
          <span className="text-[11px] text-muted-foreground">Select or create a session</span>
        </div>
      )}
    </div>
  );
}
