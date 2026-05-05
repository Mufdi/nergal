import { useRef, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  workspacesAtom,
  activeSessionIdAtom,
  activeSessionAtom,
  activeWorkspaceAtom,
  sessionLaunchModeAtom,
  freshSessionsAtom,
} from "@/stores/workspace";
import { focusZoneAtom } from "@/stores/shortcuts";
import * as terminalService from "./terminalService";

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
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    terminalService.setHost(hostRef.current);
    return () => terminalService.setHost(null);
  }, []);

  useEffect(() => {
    if (!activeSessionId || !activeSession || !activeWorkspace) return;
    if (activeSession.status === "completed") return;

    let mode: "new" | "continue";
    if (launchModes[activeSessionId]) {
      mode = launchModes[activeSessionId];
    } else if (freshSessions.has(activeSessionId)) {
      mode = "new";
    } else {
      mode = "continue";
    }

    const cwd = activeSession.worktree_path ?? activeWorkspace.repo_path;
    void terminalService.show(activeSessionId, cwd, mode);
  }, [activeSessionId, activeSession, activeWorkspace, launchModes, freshSessions]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => terminalService.fitActive());
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  // When the global focus zone flips to "terminal" (e.g. Ctrl+Ñ toggle,
  // pane cycle), snap focus back into the terminal's input surface so the
  // user doesn't need to click to start typing.
  useEffect(() => {
    if (focusZone === "terminal") {
      terminalService.focusActive();
    }
  }, [focusZone]);

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
      {(!hasAnySessions || !activeSessionId) && (
        <div className="flex h-full items-center justify-center">
          <span className="text-[11px] text-muted-foreground">Select or create a session</span>
        </div>
      )}
    </div>
  );
}
