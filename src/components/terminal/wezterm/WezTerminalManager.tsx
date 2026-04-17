import { useEffect, useRef } from "react";
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
import * as wezTerminalService from "./wezTerminalService";

/// Drop-in replacement for the legacy `TerminalManager` — same props/atoms,
/// same React lifecycle; the rendering internals switch to the wezterm-term
/// backed canvas renderer. Gated by `experimental_wezterm_terminal` in
/// [`TerminalManager`]; this component is never mounted while the flag is
/// off, so no canvas / keydown handlers leak into the legacy path.
export function WezTerminalManager() {
  const workspaces = useAtomValue(workspacesAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const activeSession = useAtomValue(activeSessionAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const launchModes = useAtomValue(sessionLaunchModeAtom);
  const freshSessions = useAtomValue(freshSessionsAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    wezTerminalService.setHost(hostRef.current);
    return () => wezTerminalService.setHost(null);
  }, []);

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
    void wezTerminalService.show(activeSessionId, cwd, mode);
  }, [activeSessionId, activeSession, activeWorkspace, launchModes, freshSessions]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => wezTerminalService.fitActive());
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  const hasAnySessions = workspaces.some((ws) => ws.sessions.length > 0);

  return (
    <div
      ref={hostRef}
      className="relative h-full w-full"
      onMouseDown={() => setFocusZone("terminal")}
    >
      {(!hasAnySessions || !activeSessionId) && (
        <div className="flex h-full items-center justify-center">
          <span className="text-[11px] text-muted-foreground">Select or create a session</span>
        </div>
      )}
    </div>
  );
}
