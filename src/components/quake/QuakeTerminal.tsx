import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Plus, X } from "lucide-react";
import {
  quakeOpenMapAtom,
  quakeHeightAtom,
  quakeShellsAtom,
  activeQuakeShellAtom,
  addAdHocShell,
  removeShell,
} from "@/stores/quake";
import {
  activeSessionAtom,
  activeSessionIdAtom,
  activeWorkspaceAtom,
} from "@/stores/workspace";
import { focusZoneAtom } from "@/stores/shortcuts";
import { configAtom } from "@/stores/config";
import { useFocusPulse } from "@/hooks/useFocusPulse";
import { appStore } from "@/stores/jotaiStore";
import * as terminalService from "@/components/terminal/terminalService";

const MIN_HEIGHT = 150;
const SLIDE_MS = 200;

/// Full-width drop-down overlay holding the active session's auxiliary
/// shells. Toggled with Ctrl+}; stays visible when focus leaves so logs
/// remain readable while typing elsewhere.
export function QuakeTerminal() {
  const openMap = useAtomValue(quakeOpenMapAtom);
  const [height, setHeight] = useAtom(quakeHeightAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const open = activeSessionId ? (openMap[activeSessionId] ?? false) : false;
  const activeSession = useAtomValue(activeSessionAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const shellsMap = useAtomValue(quakeShellsAtom);
  const [activeShellMap, setActiveShellMap] = useAtom(activeQuakeShellAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const focusPulseEnabled = useAtomValue(configAtom).panel_focus_pulse;
  const hostRef = useRef<HTMLDivElement>(null);

  const isFocused = focusZone === "quake";
  const pulsing = useFocusPulse(isFocused);
  const showAccent = focusPulseEnabled ? pulsing : isFocused;
  const borderClass = showAccent ? "border-primary" : "border-border";

  // Quake slide: animate with transform only — it's GPU-composited and
  // doesn't fire the host's ResizeObserver, so the canvas never refits
  // mid-animation. The component stays mounted through the slide-up
  // (`rendered` outlives `open` by SLIDE_MS); session switches snap instead
  // of animating, so a stale session's content never plays an exit.
  const [rendered, setRendered] = useState(open);
  const [slidIn, setSlidIn] = useState(open);
  const prevSidRef = useRef(activeSessionId);
  useEffect(() => {
    const switched = prevSidRef.current !== activeSessionId;
    prevSidRef.current = activeSessionId;
    if (open) {
      setRendered(true);
      // Double rAF: let the browser paint the off-screen position first so
      // the transition has a starting frame.
      const id = requestAnimationFrame(() =>
        requestAnimationFrame(() => setSlidIn(true)),
      );
      return () => cancelAnimationFrame(id);
    }
    setSlidIn(false);
    if (switched) {
      setRendered(false);
      return;
    }
    const t = setTimeout(() => setRendered(false), SLIDE_MS);
    return () => clearTimeout(t);
  }, [open, activeSessionId]);

  const shells = activeSessionId ? (shellsMap[activeSessionId] ?? []) : [];
  const activeShellId = activeSessionId
    ? (activeShellMap[activeSessionId] ?? shells[0]?.shellId ?? null)
    : null;
  const cwd = activeSession?.worktree_path ?? activeWorkspace?.repo_path ?? null;

  // activeSessionId is a dep because the component renders null without one:
  // the host div detaches while the effect (keyed on open alone) would keep
  // pointing at the dead node after a session restore.
  useEffect(() => {
    if (!open) return;
    terminalService.setHost(hostRef.current, "quake");
    return () => terminalService.setHost(null, "quake");
  }, [open, activeSessionId]);

  useEffect(() => {
    if (!open || !activeSessionId || !cwd) return;
    if (!activeShellId) {
      terminalService.hideAll("quake");
      return;
    }
    const shell = shells.find((sh) => sh.shellId === activeShellId);
    if (!shell) return;
    // The component never auto-runs: environment shells auto-run from the
    // session-creation flow; everything spawned here (re-open, ad-hoc) is
    // pre-fill-only. spawn_aux_shell is idempotent for already-live shells.
    void terminalService
      .showShell({
        sessionId: activeSessionId,
        shellId: shell.shellId,
        cwd,
        command: shell.command,
        autorun: false,
      })
      .then(() => {
        if (appStore.get(focusZoneAtom) === "quake") {
          terminalService.focusActive("quake");
        }
      });
  }, [open, activeSessionId, activeShellId, shells, cwd]);

  useEffect(() => {
    if (!open) return;
    const host = hostRef.current;
    if (!host) return;
    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => terminalService.fitActive("quake"));
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, [open]);

  if (!rendered || !activeSessionId) return null;

  function selectShell(shellId: string) {
    if (!activeSessionId) return;
    setActiveShellMap((prev) => ({ ...prev, [activeSessionId]: shellId }));
    setFocusZone("quake");
  }

  function startResize(e: React.MouseEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const startHeight = height;
    function onMove(ev: MouseEvent) {
      const next = startHeight + (ev.clientY - startY);
      const max = Math.floor(window.innerHeight * 0.8);
      setHeight(Math.min(max, Math.max(MIN_HEIGHT, next)));
    }
    function onUp() {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  return (
    <div
      data-focus-zone="quake"
      style={{ height }}
      // z-[45]: above the BrowserHost iframe (portaled to body at z-40),
      // below dialogs/zen (z-50+).
      className={`absolute inset-x-2 top-0 z-[45] flex flex-col overflow-hidden rounded-b-lg border-2 ${borderClass} bg-terminal-surface shadow-xl cluihud-panel-focus transition-transform duration-200 ease-out will-change-transform ${
        slidIn ? "translate-y-0" : "translate-y-[calc(-100%_-_8px)]"
      }`}
      onMouseDown={() => {
        setFocusZone("quake");
        terminalService.focusActive("quake");
      }}
    >
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-border/60 bg-card/60 px-2">
        {shells.map((sh) => {
          const active = sh.shellId === activeShellId;
          return (
            <span
              key={sh.shellId}
              className={`group flex items-center gap-1 rounded px-2 py-0.5 font-mono text-[11px] transition-colors ${
                active
                  ? "bg-secondary text-foreground"
                  : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
              }`}
            >
              <button onClick={() => selectShell(sh.shellId)}>{sh.label}</button>
              <button
                aria-label={`Close ${sh.label}`}
                onClick={(e) => {
                  e.stopPropagation();
                  removeShell(activeSessionId, sh.shellId, true);
                }}
                className="opacity-0 transition-opacity group-hover:opacity-100"
              >
                <X className="size-3" />
              </button>
            </span>
          );
        })}
        <button
          aria-label="New shell"
          onClick={() => addAdHocShell(activeSessionId)}
          className="flex size-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
        >
          <Plus className="size-3.5" />
        </button>
        <span className="ml-auto text-[10px] text-muted-foreground/50 select-none">
          Ctrl+&#125; hide
        </span>
      </div>

      <div ref={hostRef} className="relative flex-1 overflow-hidden p-1">
        {shells.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 select-none">
            <span className="text-[11px] text-muted-foreground/60">No shells</span>
            <button
              onClick={() => addAdHocShell(activeSessionId)}
              className="rounded border border-border px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            >
              New shell
            </button>
          </div>
        )}
      </div>

      <div
        onMouseDown={startResize}
        className="h-1.5 shrink-0 cursor-ns-resize bg-border/40 transition-colors hover:bg-primary/50"
        aria-label="Resize quake terminal"
      />
    </div>
  );
}
