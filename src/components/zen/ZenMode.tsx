import { useState, useEffect, useCallback, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { zenModeAtom, closeZenModeAtom, zenModeNavigateAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import { activeSessionIdAtom } from "@/stores/workspace";
import { DiffView } from "@/components/plan/DiffView";
import { FilesChip } from "@/components/git/chips/FilesChip";
import { HistoryChip } from "@/components/git/chips/HistoryChip";
import { ConflictsPanel } from "@/components/git/ConflictsPanel";
import { invoke } from "@/lib/tauri";
import { X, ChevronLeft, ChevronRight } from "lucide-react";

type SidebarTab = "changes" | "history";
type ZenZone = "viewer" | "sidebar";

/// Full-screen diff review overlay with git sidebar.
export function GitFullView() {
  const state = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const setConflictsZen = useSetAtom(conflictsZenOpenAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const close = useSetAtom(closeZenModeAtom);
  const navigate = useSetAtom(zenModeNavigateAtom);
  const [sidebarTab, setSidebarTab] = useState<SidebarTab>("changes");
  const [zone, setZone] = useState<ZenZone>("viewer");
  const viewerRef = useRef<HTMLDivElement>(null);
  const sidebarRef = useRef<HTMLDivElement>(null);

  const closeAll = useCallback(() => {
    if (conflictsZen) setConflictsZen(false);
    else close();
  }, [conflictsZen, setConflictsZen, close]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!state.open && !conflictsZen) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeAll();
      return;
    }
    if (!state.open) return;
    // Alt+←/→: rebound while Zen is open. Outside Zen these toggle app focus
    // zones (sidebar/terminal/panel); inside Zen we steal them so the user
    // moves between the diff viewer and the git sidebar without leaking the
    // keystroke to the underlying app — same shortcut, same hand position,
    // contextual meaning.
    if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight") && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      setZone((z) => (z === "viewer" ? "sidebar" : "viewer"));
      return;
    }
    // Alt+↑/↓: outside Zen these drive the global navigateItems within the
    // active focus zone (sidebar sessions, panel rows). Inside Zen the
    // panel underneath is hidden behind the overlay — letting that
    // shortcut through means a stray Alt+↑ moves sessions/tabs invisibly.
    // Swallow it; chunk/file nav uses j/k/arrows without the modifier.
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown") && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
      return;
    }
  }, [state.open, conflictsZen, closeAll]);

  useEffect(() => {
    // Capture phase + stopPropagation so the global shortcut registry never
    // sees Alt+arrow combos while Zen is open. Without capture the shortcut
    // hook (window bubble listener) fires first and the focus zone leaks.
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [handleKeyDown]);

  // Whenever the active Zen zone changes (or Zen opens), move DOM focus there
  // so the right keyboard listener (DiffView capture vs FilesChip window)
  // owns the next keystroke. The sidebar specifically focuses the inner chip
  // container so FilesChip's inZen scope-check (target ∈ chip) passes.
  useEffect(() => {
    if (!state.open) return;
    const root = zone === "viewer" ? viewerRef.current : sidebarRef.current;
    if (!root) return;
    const focusTarget =
      zone === "viewer"
        ? (root.querySelector("[data-scrollable]") as HTMLElement | null) ?? root
        : (root.querySelector("[data-files-chip]") as HTMLElement | null) ?? root;
    requestAnimationFrame(() => focusTarget.focus());
  }, [zone, state.open, state.filePath, sidebarTab]);

  if (conflictsZen && sessionId) {
    return (
      <div className="fixed inset-0 z-40 flex overflow-hidden" role="dialog" aria-label="Conflict resolution full view">
        <div className="absolute inset-0 bg-background/60 backdrop-blur-md" onClick={closeAll} />
        <div className="relative z-10 flex min-w-0 flex-1 flex-col m-3 overflow-hidden rounded-lg bg-card/95 border border-border">
          <ConflictsPanel sessionId={sessionId} inZen onToggleZen={closeAll} />
        </div>
      </div>
    );
  }

  if (!state.open || !state.filePath || !state.sessionId) return null;

  const fileName = state.filePath.split("/").pop() ?? state.filePath;

  return (
    <div
      className="fixed inset-0 z-40 flex overflow-hidden"
      role="dialog"
      aria-label="Git full diff review"
    >
      {/* Blur backdrop */}
      <div
        className="absolute inset-0 bg-background/60 backdrop-blur-md"
        onClick={close}
      />

      {/* Diff content area */}
      <div
        ref={viewerRef}
        tabIndex={-1}
        onMouseDown={() => setZone("viewer")}
        className={`relative z-10 flex min-w-0 flex-1 flex-col m-3 mr-0 overflow-hidden outline-none ring-1 transition-colors ${
          zone === "viewer" ? "ring-primary/40" : "ring-transparent"
        }`}
      >
        {/* Header */}
        <div className="flex h-9 shrink-0 items-center justify-between rounded-t-lg bg-card/95 border border-border px-3">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => navigate("prev")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Previous file"
            >
              <ChevronLeft className="size-4" />
            </button>
            <span className="text-xs text-foreground font-medium truncate">{fileName}</span>
            <span className="text-[10px] text-muted-foreground">
              ({state.currentIndex + 1}/{state.files.length})
            </span>
            <button
              type="button"
              onClick={() => navigate("next")}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Next file"
            >
              <ChevronRight className="size-4" />
            </button>
          </div>
          <div className="flex items-center gap-1">
            <span className="text-[10px] text-muted-foreground">Alt+←/→ to switch · Esc to close</span>
            <button
              type="button"
              onClick={close}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground hover:bg-secondary"
              aria-label="Close"
            >
              <X className="size-4" />
            </button>
          </div>
        </div>

        {/* Diff viewer — side by side */}
        <div className="flex-1 overflow-hidden rounded-b-lg bg-card/95 border border-t-0 border-border">
          <DiffView key={state.filePath} filePath={state.filePath} sessionId={state.sessionId} sideBySide />
        </div>
      </div>

      {/* Git sidebar with tabs */}
      <div
        ref={sidebarRef}
        tabIndex={-1}
        onMouseDown={() => setZone("sidebar")}
        className={`relative z-10 w-72 shrink-0 flex flex-col m-3 ml-1.5 overflow-hidden rounded-lg bg-card/95 border outline-none ring-1 transition-colors ${
          zone === "sidebar" ? "border-primary/40 ring-primary/40" : "border-border ring-transparent"
        }`}
      >
        {/* Tab bar */}
        <div className="flex shrink-0 border-b border-border/50">
          {(["changes", "history"] as const).map((tab) => (
            <button
              key={tab}
              type="button"
              onClick={() => setSidebarTab(tab)}
              className={`flex-1 py-1.5 text-center text-[10px] font-medium uppercase tracking-wider transition-colors ${
                sidebarTab === tab
                  ? "text-foreground border-b-2 border-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>

        {/* Tab content */}
        <div className="flex-1 overflow-hidden">
          {sidebarTab === "changes" ? (
            <GitPanelChangesOnly sessionId={state.sessionId} />
          ) : (
            <GitPanelHistoryOnly sessionId={state.sessionId} />
          )}
        </div>
      </div>
    </div>
  );
}

/// Files chip rendered standalone in Zen sidebar — no chip strip, no shell.
/// Loads ahead count itself since GitPanel shell is bypassed.
function GitPanelChangesOnly({ sessionId }: { sessionId: string }) {
  const [ahead, setAhead] = useState(0);
  useEffect(() => {
    invoke<{ ahead: number }>("get_session_git_info", { sessionId })
      .then((info) => setAhead(info.ahead))
      .catch(() => {});
  }, [sessionId]);
  return (
    <div className="h-full overflow-hidden">
      <FilesChip sessionId={sessionId} ahead={ahead} inZen />
    </div>
  );
}

/// History chip rendered standalone in Zen sidebar.
function GitPanelHistoryOnly({ sessionId }: { sessionId: string }) {
  return (
    <div className="h-full overflow-hidden">
      <HistoryChip sessionId={sessionId} />
    </div>
  );
}

// Re-export for backwards compat with Workspace import
export { GitFullView as ZenMode };
