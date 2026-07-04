import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from "react";

export interface Region {
  start: number;
  sep: number;
  end: number;
  oursLines: string[];
  theirsLines: string[];
}

export type RegionChoice = "ours" | "theirs" | "both";

const START_RE = /^<{7}(\s|$)/;
const SEP_RE = /^={7}(\s|$)/;
const END_RE = /^>{7}(\s|$)/;

export function parseRegions(text: string): Region[] {
  const lines = text.split("\n");
  const regions: Region[] = [];
  let start = -1;
  let sep = -1;
  for (let i = 0; i < lines.length; i++) {
    if (START_RE.test(lines[i])) { start = i; sep = -1; }
    else if (SEP_RE.test(lines[i]) && start !== -1) { sep = i; }
    else if (END_RE.test(lines[i]) && start !== -1 && sep !== -1) {
      regions.push({
        start, sep, end: i,
        oursLines: lines.slice(start + 1, sep),
        theirsLines: lines.slice(sep + 1, i),
      });
      start = -1; sep = -1;
    }
  }
  return regions;
}

export function applyChoice(text: string, region: Region, choice: RegionChoice): string {
  const lines = text.split("\n");
  const replacement =
    choice === "ours" ? region.oursLines
    : choice === "theirs" ? region.theirsLines
    : [...region.oursLines, ...region.theirsLines];
  lines.splice(region.start, region.end - region.start + 1, ...replacement);
  return lines.join("\n");
}

/// Used by the merged-pane save guard — saving must be blocked while any
/// conflict marker survives in the text.
export function hasConflictMarkers(text: string): boolean {
  return text.split("\n").some((line) => START_RE.test(line) || SEP_RE.test(line) || END_RE.test(line));
}

/// Region-cursor advance: clamps at the ends (parity with DiffView hunk nav).
/// Only called with count > 0 by the keyboard handler below.
export function clampRegionIndex(current: number, count: number, direction: "prev" | "next"): number {
  return direction === "next" ? Math.min(current + 1, count - 1) : Math.max(current - 1, 0);
}

/// File-picker cursor advance: wraps around (parity with PrViewer's picker).
export function wrapPickerIndex(current: number, count: number, direction: "prev" | "next"): number {
  if (count === 0) return 0;
  return direction === "next" ? (current + 1) % count : (current - 1 + count) % count;
}

export interface UseConflictResolutionArgs {
  /// Current merged text and whether the conflict data has finished loading
  /// (mirrors `stateMap[key]`'s presence — the panel never sets `merged`
  /// without `loaded: true` in the same write, so a single flag covers both
  /// the region-parsing gate and the accept/reject gate).
  merged: string;
  loaded: boolean;
  updateMerged: (next: string) => void;
  files: string[];
  path: string;
  listenerActive: boolean;
  onNavFile?: (direction: "prev" | "next") => void;
  onPickFile?: (path: string) => void;
  toggleSyncScroll: () => void;
  onSave: () => void;
  onReset: () => void;
  onAskClaude: () => void;
  onAcceptAll: (choice: RegionChoice, regionCount: number) => void;
}

export interface UseConflictResolutionResult {
  regions: Region[];
  focusedRegion: number;
  setFocusedRegion: Dispatch<SetStateAction<number>>;
  scrollNonce: number;
  applyRegion: (regionIdx: number, choice: RegionChoice) => void;
  acceptAll: (choice: RegionChoice) => void;
  pickerOpen: boolean;
  setPickerOpen: Dispatch<SetStateAction<boolean>>;
  pickerCursor: number;
  setPickerCursor: Dispatch<SetStateAction<number>>;
}

/// Owns the conflict-resolution state machine for one ConflictView instance:
/// region cursor + navigation, accept/reject application, the file-picker
/// overlay's cursor, and the keyboard dispatcher that drives all of it.
/// EditorView instances are NOT threaded through here — none of this logic
/// touches CodeMirror. Pane mounting and the 3-way scroll-sync effect stay
/// owned by ConflictView (see design.md D2: those views must remain
/// `useState`, not refs, so ConnectorStrip re-renders when they mount).
export function useConflictResolution({
  merged,
  loaded,
  updateMerged,
  files,
  path,
  listenerActive,
  onNavFile,
  onPickFile,
  toggleSyncScroll,
  onSave,
  onReset,
  onAskClaude,
  onAcceptAll,
}: UseConflictResolutionArgs): UseConflictResolutionResult {
  const [focusedRegion, setFocusedRegion] = useState<number>(0);
  /// File picker state — mirrors PrViewer's pattern. The picker swaps which
  /// file the panel shows; opened via Ctrl+Shift+K (global) or the chevrons.
  /// j/k drives the cursor without committing; Enter commits + closes; Esc
  /// closes without changing the file. Cursor seeds from the active file
  /// every time the picker opens so navigation starts in a useful spot.
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerCursor, setPickerCursor] = useState(0);

  const regions = useMemo(() => (loaded ? parseRegions(merged) : []), [merged, loaded]);

  /// When the user navigates regions (via j/k, header-row click, or chevron
  /// nav), the nonce bumps so the panes re-scroll to the corresponding
  /// region even if the same region is picked twice in a row.
  const [scrollNonce, setScrollNonce] = useState(0);
  useEffect(() => {
    setScrollNonce((n) => n + 1);
  }, [focusedRegion]);

  useEffect(() => {
    if (focusedRegion >= regions.length) setFocusedRegion(Math.max(0, regions.length - 1));
  }, [regions.length, focusedRegion]);

  const applyRegion = useCallback((regionIdx: number, choice: RegionChoice) => {
    if (!loaded) return;
    const regs = parseRegions(merged);
    const region = regs[regionIdx];
    if (!region) return;
    updateMerged(applyChoice(merged, region, choice));
  }, [loaded, merged, updateMerged]);

  /// Accept the same choice for every remaining conflict region. Iterates
  /// bottom-up so each splice doesn't shift indices we haven't visited yet.
  const acceptAll = useCallback((choice: RegionChoice) => {
    if (!loaded) return;
    let next = merged;
    const regs = parseRegions(next);
    if (regs.length === 0) return;
    for (let i = regs.length - 1; i >= 0; i--) {
      next = applyChoice(next, regs[i], choice);
    }
    updateMerged(next);
    onAcceptAll(choice, regs.length);
  }, [loaded, merged, updateMerged, onAcceptAll]);

  /// Global Ctrl+Shift+K → toggle file picker. Same dispatcher PrViewer uses,
  /// so muscle memory carries between panels. Opening the picker seeds the
  /// cursor at the active file's index so j/k starts in context.
  useEffect(() => {
    if (!listenerActive) return;
    function onToggle() {
      setPickerOpen((open) => {
        if (!open) {
          const idx = files.indexOf(path);
          setPickerCursor(idx >= 0 ? idx : 0);
        }
        return !open;
      });
    }
    document.addEventListener("nergal:toggle-file-picker", onToggle);
    return () => document.removeEventListener("nergal:toggle-file-picker", onToggle);
  }, [listenerActive, files, path]);

  useEffect(() => {
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inEditor = target?.tagName === "TEXTAREA"
        || target?.tagName === "INPUT"
        || !!target?.closest(".cm-editor");
      // Picker open: j/k drives the cursor, Enter commits, Esc closes. Steal
      // these keys away from chunk navigation while the picker has the floor.
      if (pickerOpen) {
        if (e.code === "Escape" || e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          setPickerOpen(false);
          return;
        }
        if (files.length === 0) return;
        if (e.code === "KeyJ" || e.code === "ArrowDown") {
          e.preventDefault();
          e.stopPropagation();
          setPickerCursor((i) => wrapPickerIndex(i, files.length, "next"));
          return;
        }
        if (e.code === "KeyK" || e.code === "ArrowUp") {
          e.preventDefault();
          e.stopPropagation();
          setPickerCursor((i) => wrapPickerIndex(i, files.length, "prev"));
          return;
        }
        if (e.code === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          const pick = files[pickerCursor];
          if (pick && onPickFile) onPickFile(pick);
          setPickerOpen(false);
          return;
        }
        return;
      }
      // Ctrl+←/→ — file prev/next across the conflicted-files list. Owner
      // wires the actual move via onNavFile so this stays editor-agnostic.
      if (onNavFile && e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
        if (e.code === "ArrowLeft") {
          e.preventDefault();
          e.stopPropagation();
          onNavFile("prev");
          return;
        }
        if (e.code === "ArrowRight") {
          e.preventDefault();
          e.stopPropagation();
          onNavFile("next");
          return;
        }
      }
      if (!inEditor && regions.length > 0) {
        // Arrow or J/K navigation between regions (parity with DiffView hunk nav).
        if ((e.key === "ArrowDown" || e.key === "j" || e.key === "J") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          setFocusedRegion((i) => clampRegionIndex(i, regions.length, "next"));
          return;
        }
        if ((e.key === "ArrowUp" || e.key === "k" || e.key === "K") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          setFocusedRegion((i) => clampRegionIndex(i, regions.length, "prev"));
          return;
        }
        if (e.key === "o" || e.key === "O") { e.preventDefault(); applyRegion(focusedRegion, "ours"); return; }
        if (e.key === "t" || e.key === "T") { e.preventDefault(); applyRegion(focusedRegion, "theirs"); return; }
        if (e.key === "b" || e.key === "B") { e.preventDefault(); applyRegion(focusedRegion, "both"); return; }
        if (e.key === "s" || e.key === "S") { e.preventDefault(); toggleSyncScroll(); return; }
      }
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey) return;
      // Ctrl+Shift+O / Ctrl+Shift+T: accept ALL regions of one side. Mirrors
      // IntelliJ's "Apply Non-Conflicting Changes from Left/Right Side" but
      // applied to every conflict, since git's <<<<<<< blocks are by
      // definition conflicting. Lowercase O/T already handle per-region.
      if (e.code === "KeyO") { e.preventDefault(); acceptAll("ours"); }
      else if (e.code === "KeyT") { e.preventDefault(); acceptAll("theirs"); }
      else if (e.code === "KeyZ") { e.preventDefault(); onReset(); }
      else if (e.key === "Enter") { e.preventDefault(); onSave(); }
    }
    function onResolveActive() { onAskClaude(); }
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("nergal:resolve-conflict-active-tab", onResolveActive);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("nergal:resolve-conflict-active-tab", onResolveActive);
    };
  }, [regions, focusedRegion, applyRegion, acceptAll, onReset, onSave, onAskClaude, listenerActive, onNavFile, pickerOpen, pickerCursor, files, onPickFile, toggleSyncScroll]);

  return {
    regions,
    focusedRegion,
    setFocusedRegion,
    scrollNonce,
    applyRegion,
    acceptAll,
    pickerOpen,
    setPickerOpen,
    pickerCursor,
    setPickerCursor,
  };
}
