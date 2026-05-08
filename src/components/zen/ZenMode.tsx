import { useEffect, useCallback, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  zenModeAtom,
  zenActiveZoneAtom,
  closeZenModeAtom,
  zenModeNavigateAtom,
  zenModeSelectFileAtom,
  prZenAtom,
} from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import {
  prAnnotationsKey,
  prFilesCacheAtom,
  selectedPrFileAtom,
} from "@/stores/git";
import { activeSessionIdAtom } from "@/stores/workspace";
import { DiffView } from "@/components/plan/DiffView";
import { ConflictsPanel } from "@/components/git/ConflictsPanel";
import { PrViewer } from "@/components/git/PrViewer";
import { X, ChevronLeft, ChevronRight } from "lucide-react";

/// Full-screen diff review overlay with git sidebar.
export function GitFullView() {
  const state = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const setConflictsZen = useSetAtom(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const close = useSetAtom(closeZenModeAtom);
  const navigate = useSetAtom(zenModeNavigateAtom);
  const selectFile = useSetAtom(zenModeSelectFileAtom);
  const [zone, setZone] = useAtom(zenActiveZoneAtom);

  const closeAll = useCallback(() => {
    if (conflictsZen) setConflictsZen(false);
    else close();
  }, [conflictsZen, setConflictsZen, close]);

  // Any of the three open atoms means Zen is overlaid. We treat them as a
  // single boolean for keyboard gating; the render branches differentiate.
  const anyOverlayOpen = state.open || conflictsZen || prZen !== null;

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (!anyOverlayOpen) return;
    // Field-aware bail. Without this, Esc inside the PR annotation textarea
    // (or any input the user opens within Zen) closes the entire overlay
    // instead of letting the field cancel its own draft. The annotation
    // textarea handles Esc itself via its onKeyDown — we just need to step
    // out of its way.
    const target = e.target as HTMLElement | null;
    const inField = target?.tagName === "INPUT"
      || target?.tagName === "TEXTAREA"
      || !!target?.closest(".cm-editor")
      || target?.getAttribute("contenteditable") === "true";
    if (inField) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeAll();
      return;
    }
    if (!state.open && !prZen) return;
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
  }, [state.open, conflictsZen, prZen, anyOverlayOpen, closeAll, setZone]);

  useEffect(() => {
    // Capture phase + stopPropagation so the global shortcut registry never
    // sees Alt+arrow combos while Zen is open. Without capture the shortcut
    // hook (window bubble listener) fires first and the focus zone leaks.
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [handleKeyDown]);

  // Reset zone to viewer only when Zen opens, not when filePath changes within
  // an already-open Zen. Resetting on filePath change stole focus back to the
  // viewer every time the user picked a file in the sidebar — including via
  // Enter from j/k navigation, which made sidebar-driven file switching feel
  // broken. PR Zen avoids this by storing selection in a separate atom; we
  // achieve the same effect by gating the reset to open transitions only.
  useEffect(() => {
    if (state.open || prZen) setZone("viewer");
  }, [state.open, prZen, setZone]);

  // Diff Zen sidebar cursor — separate from `currentIndex` so arrows move a
  // hover ring through the file list without firing the diff fetch on every
  // keystroke. Enter commits to `selectFile`. Snaps to currentIndex whenever
  // the user enters the sidebar zone so the starting position matches what
  // the viewer is showing.
  const [sidebarCursor, setSidebarCursor] = useState(0);
  useEffect(() => {
    if (zone === "sidebar") setSidebarCursor(state.currentIndex);
  }, [zone, state.currentIndex]);
  useEffect(() => {
    if (!state.open) return;
    if (zone !== "sidebar") return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (state.files.length === 0) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ") {
        e.preventDefault();
        e.stopPropagation();
        setSidebarCursor((i) => Math.min(state.files.length - 1, i + 1));
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        e.stopPropagation();
        setSidebarCursor((i) => Math.max(0, i - 1));
        return;
      }
      if (e.code === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        const target = state.files[sidebarCursor];
        if (target) selectFile(target);
        return;
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [zone, state.open, state.files, sidebarCursor, selectFile]);

  if (conflictsZen && sessionId) {
    return (
      <div className="fixed inset-0 z-40 flex overflow-hidden" role="dialog" aria-label="Conflict resolution full view">
        <div className="absolute inset-0 bg-background/60 cluihud-blur-md" onClick={closeAll} />
        <div className="relative z-10 flex min-w-0 flex-1 flex-col m-3 overflow-hidden rounded-lg bg-card/95 border-2 border-border">
          <ConflictsPanel sessionId={sessionId} inZen onToggleZen={closeAll} />
        </div>
      </div>
    );
  }

  if (prZen) {
    return (
      <div className="fixed inset-0 z-40 flex overflow-hidden" role="dialog" aria-label={`PR #${prZen.prNumber} full view`}>
        <div className="absolute inset-0 bg-background/60 cluihud-blur-md" onClick={closeAll} />

        {/* Viewer */}
        <div
          onMouseDown={() => setZone("viewer")}
          className={`relative z-10 flex min-w-0 flex-1 flex-col m-3 mr-0 overflow-hidden rounded-lg bg-card/95 border-2 cluihud-panel-focus ${
            zone === "viewer" ? "border-primary" : "border-border"
          }`}
        >
          <PrViewer data={prZen} inZen />
        </div>

        {/* PR files sidebar */}
        <div
          onMouseDown={() => setZone("sidebar")}
          className={`relative z-10 w-72 shrink-0 flex flex-col m-3 ml-1.5 overflow-hidden rounded-lg bg-card/95 border-2 cluihud-panel-focus ${
            zone === "sidebar" ? "border-primary" : "border-border"
          }`}
        >
          <PrZenSidebar workspaceId={prZen.workspaceId} prNumber={prZen.prNumber} sidebarActive={zone === "sidebar"} />
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
        className="absolute inset-0 bg-background/60 cluihud-blur-md"
        onClick={close}
      />

      {/* Diff content area */}
      <div
        onMouseDown={() => setZone("viewer")}
        className={`relative z-10 flex min-w-0 flex-1 flex-col m-3 mr-0 overflow-hidden rounded-lg border-2 bg-card/95 cluihud-panel-focus ${
          zone === "viewer" ? "border-primary" : "border-border"
        }`}
      >
        {/* Header */}
        <div className="flex h-9 shrink-0 items-center justify-between border-b border-border/50 px-3">
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
        <div className="flex-1 overflow-hidden">
          <DiffView
            key={state.filePath}
            filePath={state.filePath}
            sessionId={state.sessionId}
            sideBySide
            inZen
            onNavFile={(direction) => navigate(direction)}
          />
        </div>
      </div>

      {/* Files sidebar — mirrors the file picker the user sees in the Diff
          panel. The previous Zen sidebar rendered FilesChip's full
          stage/track UI, which is irrelevant when the user is reviewing
          a diff: here they only need to switch which file the viewer shows. */}
      <div
        onMouseDown={() => setZone("sidebar")}
        className={`relative z-10 w-72 shrink-0 flex flex-col m-3 ml-1.5 overflow-hidden rounded-lg bg-card/95 border-2 cluihud-panel-focus ${
          zone === "sidebar" ? "border-primary" : "border-border"
        }`}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-border/50 px-3 py-1.5">
          <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
            Files ({state.files.length})
          </span>
          <span className="text-[9px] text-muted-foreground/50">
            {zone === "sidebar" ? "j/k move · Enter pick" : "Alt+→ to focus"}
          </span>
        </div>
        <div className="flex-1 overflow-y-auto">
          {state.files.map((fp, i) => {
            const isCurrent = fp === state.filePath;
            const isCursor = zone === "sidebar" && sidebarCursor === i;
            const filename = fp.split("/").pop() ?? fp;
            const dir = fp.includes("/") ? fp.slice(0, fp.lastIndexOf("/")) : "";
            return (
              <button
                key={fp}
                type="button"
                onMouseEnter={() => { if (zone === "sidebar") setSidebarCursor(i); }}
                onClick={() => selectFile(fp)}
                ref={(el) => { if (el && (isCursor || (zone !== "sidebar" && isCurrent))) el.scrollIntoView({ block: "nearest" }); }}
                className={`flex w-full flex-col gap-0 px-3 py-1 text-left transition-colors border-l-2 ${
                  isCursor
                    ? "border-l-orange-500 bg-orange-500/15"
                    : isCurrent
                      ? "border-l-orange-500/40 bg-orange-500/5"
                      : "border-l-transparent hover:bg-secondary/30"
                }`}
              >
                <span className={`truncate font-mono text-[11px] ${isCurrent ? "text-foreground" : "text-foreground/80"}`}>
                  {filename}
                </span>
                {dir && (
                  <span className="truncate font-mono text-[9px] text-muted-foreground/60">
                    {dir}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

/// PR Zen sidebar — reads the parsed PR file list from `prFilesCacheAtom`
/// (populated by PrViewer when it parses the diff) and the active selection
/// from `selectedPrFileAtom`. Click selects; arrow/j/k navigate when this
/// zone owns keyboard input. We deliberately don't re-fetch the diff here —
/// the viewer is already loading it, and shared state keeps both panes
/// instantly in sync as the user picks files.
function PrZenSidebar({ workspaceId, prNumber, sidebarActive }: { workspaceId: string; prNumber: number; sidebarActive: boolean }) {
  const key = prAnnotationsKey(workspaceId, prNumber);
  const filesCache = useAtomValue(prFilesCacheAtom);
  const [selectedMap, setSelectedMap] = useAtom(selectedPrFileAtom);
  const files = filesCache[key] ?? [];
  const selected = selectedMap[key] ?? null;
  const [cursor, setCursor] = useState(0);

  // Snap cursor to the active file whenever the user enters the sidebar zone
  // so j/k starts from "where the viewer is" rather than the top of the list.
  useEffect(() => {
    if (!sidebarActive) return;
    const idx = selected ? files.findIndex((f) => f.path === selected) : 0;
    setCursor(idx >= 0 ? idx : 0);
  }, [sidebarActive, selected, files]);

  useEffect(() => {
    if (!sidebarActive) return;
    if (files.length === 0) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ") {
        e.preventDefault();
        e.stopPropagation();
        setCursor((i) => Math.min(files.length - 1, i + 1));
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        e.stopPropagation();
        setCursor((i) => Math.max(0, i - 1));
        return;
      }
      if (e.code === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        const pick = files[cursor];
        if (pick) setSelectedMap((prev) => ({ ...prev, [key]: pick.path }));
        return;
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [sidebarActive, files, cursor, key, setSelectedMap]);

  return (
    <>
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 px-3 py-1.5">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          PR files ({files.length})
        </span>
        <span className="text-[9px] text-muted-foreground/50">
          {sidebarActive ? "j/k move" : "Alt+→ to focus"}
        </span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {files.length === 0 ? (
          <div className="flex h-full items-center justify-center px-3">
            <span className="text-[10px] text-muted-foreground/60">Loading PR files…</span>
          </div>
        ) : (
          files.map((f, i) => {
            const isCurrent = f.path === selected;
            const isCursor = sidebarActive && cursor === i;
            const filename = f.path.split("/").pop() ?? f.path;
            const dir = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "";
            return (
              <button
                key={f.path}
                type="button"
                onMouseEnter={() => { if (sidebarActive) setCursor(i); }}
                onClick={() => setSelectedMap((prev) => ({ ...prev, [key]: f.path }))}
                ref={(el) => { if (el && (isCursor || (!sidebarActive && isCurrent))) el.scrollIntoView({ block: "nearest" }); }}
                className={`flex w-full flex-col gap-0 px-3 py-1 text-left transition-colors border-l-2 ${
                  isCursor
                    ? "border-l-orange-500 bg-orange-500/15"
                    : isCurrent
                      ? "border-l-orange-500/40 bg-orange-500/5"
                      : "border-l-transparent hover:bg-secondary/30"
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className={`min-w-0 flex-1 truncate font-mono text-[11px] ${isCurrent ? "text-foreground" : "text-foreground/80"}`}>
                    {filename}
                  </span>
                  <span className="shrink-0 font-mono text-[9px]">
                    <span className="text-green-400">+{f.adds}</span>
                    <span className="text-muted-foreground/40 px-0.5">/</span>
                    <span className="text-red-400">-{f.removes}</span>
                  </span>
                </div>
                {dir && (
                  <span className="truncate font-mono text-[9px] text-muted-foreground/60">
                    {dir}
                  </span>
                )}
              </button>
            );
          })
        )}
      </div>
    </>
  );
}

// Re-export for backwards compat with Workspace import
export { GitFullView as ZenMode };
