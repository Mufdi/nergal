import { useEffect, useCallback, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { EditorView } from "@codemirror/view";
import { EditorState, Compartment, type StateEffect } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import {
  syntaxHighlighting,
  codeFolding,
  foldEffect,
  unfoldEffect,
  foldedRanges,
} from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { rust } from "@codemirror/lang-rust";
import { invoke } from "@/lib/tauri";
import {
  conflictStateMapAtom,
  conflictKey,
  selectedConflictFileMapAtom,
  conflictIntentMapAtom,
  conflictsZenOpenAtom,
} from "@/stores/conflict";
import { activeConflictedFilesAtom, refreshConflictedFilesAtom } from "@/stores/git";
import { toastsAtom } from "@/stores/toast";
import { focusZoneAtom } from "@/stores/shortcuts";
import { zenModeAtom } from "@/stores/zenMode";
import * as terminalService from "@/components/terminal/terminalService";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import {
  AlertTriangle,
  Check,
  ChevronRight,
  Minus,
  Sparkles,
  Save,
  RotateCcw,
  Send,
  FileCheck,
  Maximize2,
  Minimize2,
} from "lucide-react";

interface Props {
  sessionId: string;
  inZen?: boolean;
  onToggleZen?: () => void;
  /// Called 1.5s after activity (conflicts/pendingMerge) drains. Replaces
  /// the legacy `closeTab("conflicts")` behavior when the panel is mounted
  /// inside a chip — the chip uses this to switch to the PRs chip.
  onResolved?: () => void;
}

interface Region {
  start: number;
  sep: number;
  end: number;
  oursLines: string[];
  theirsLines: string[];
}

const START_RE = /^<{7}(\s|$)/;
const SEP_RE = /^={7}(\s|$)/;
const END_RE = /^>{7}(\s|$)/;

function getLanguageExtension(filePath: string) {
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "tsx":
      return javascript({ typescript: true, jsx: ext === "tsx" });
    case "js":
    case "jsx":
      return javascript({ jsx: ext === "jsx" });
    case "json": return json();
    case "md":
    case "mdx": return markdown();
    case "css":
    case "scss": return css();
    case "html":
    case "htm": return html();
    case "rs": return rust();
    default: return [];
  }
}

const cmTheme = EditorView.theme({
  "&": { backgroundColor: "#0a0a0b", color: "#ededef", height: "100%", fontSize: "11px" },
  ".cm-content": { caretColor: "#f97316", fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#f97316" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "rgba(249, 115, 22, 0.2)",
  },
  ".cm-activeLine": { backgroundColor: "rgba(255, 255, 255, 0.03)" },
  ".cm-gutters": {
    backgroundColor: "#0a0a0b",
    color: "#5c5c5f",
    borderRight: "1px solid rgba(255, 255, 255, 0.08)",
  },
  ".cm-activeLineGutter": { backgroundColor: "rgba(255, 255, 255, 0.05)" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 6px 0 4px" },
  ".cm-scroller": { overflow: "auto", fontFamily: "inherit" },
}, { dark: true });

function parseRegions(text: string): Region[] {
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

function applyChoice(text: string, region: Region, choice: "ours" | "theirs" | "both"): string {
  const lines = text.split("\n");
  const replacement =
    choice === "ours" ? region.oursLines
    : choice === "theirs" ? region.theirsLines
    : [...region.oursLines, ...region.theirsLines];
  lines.splice(region.start, region.end - region.start + 1, ...replacement);
  return lines.join("\n");
}

export function ConflictsPanel({ sessionId, inZen = false, onToggleZen, onResolved }: Props) {
  const files = useAtomValue(activeConflictedFilesAtom);
  const [selectedMap, setSelectedMap] = useAtom(selectedConflictFileMapAtom);
  const selectedPath = selectedMap[sessionId] ?? files[0] ?? null;
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const [pendingMerge, setPendingMerge] = useState(false);
  const [completing, setCompleting] = useState(false);
  const addToastFn = useSetAtom(toastsAtom);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  // Conflicts has its own Zen overlay (single-pane, no sidebar). Listener
  // gating: in-Zen instance fires when its overlay is open; the inline chip
  // copy fires only when no Zen is up. Diff Zen also blocks the inline copy.
  const listenerActive = inZen
    ? conflictsZen
    : !(zenState.open || conflictsZen);

  useEffect(() => {
    if (files.length === 0) return;
    if (!selectedPath || !files.includes(selectedPath)) {
      setSelectedMap((prev) => ({ ...prev, [sessionId]: files[0] }));
    }
  }, [files, selectedPath, sessionId, setSelectedMap]);

  useEffect(() => {
    refreshConflicts(sessionId);
  }, [sessionId, refreshConflicts]);

  useEffect(() => {
    invoke<boolean>("has_pending_merge", { sessionId }).then(setPendingMerge).catch(() => setPendingMerge(false));
  }, [sessionId, files.length]);

  /// Auto-resolve transition once the panel has nothing left to do. Only
  /// fires after the panel saw activity (conflicts present or a pending
  /// merge). Disabled in Zen mode to avoid snapping the surface shut.
  /// `onResolved` routes the user away (chip mode → switch to PRs chip).
  const hadActivityRef = useRef(false);
  useEffect(() => {
    if (files.length > 0 || pendingMerge) {
      hadActivityRef.current = true;
      return;
    }
    if (!hadActivityRef.current || inZen) return;
    if (!onResolved) return;
    const t = setTimeout(onResolved, 1500);
    return () => clearTimeout(t);
  }, [files.length, pendingMerge, inZen, onResolved]);

  const completeMerge = useCallback(async () => {
    if (completing) return;
    setCompleting(true);
    try {
      await invoke<string>("complete_pending_merge", { sessionId });
      addToastFn({ message: "Merge", description: "Merge commit created. Push when ready.", type: "success" });
      invoke<boolean>("has_pending_merge", { sessionId }).then(setPendingMerge).catch(() => {});
    } catch (e) {
      addToastFn({ message: "Complete merge failed", description: String(e), type: "error" });
    } finally {
      setCompleting(false);
    }
  }, [completing, sessionId, addToastFn]);

  useEffect(() => {
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      if (!(e.ctrlKey || e.metaKey) || !e.altKey) return;
      if (e.key === "Enter" && pendingMerge && files.length === 0) {
        e.preventDefault();
        completeMerge();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [pendingMerge, files.length, completeMerge, listenerActive]);

  if (files.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex max-w-sm flex-col items-center gap-3 px-4 text-center">
          <FileCheck size={24} className="text-green-400" />
          {pendingMerge ? (
            <>
              <span className="text-[11px] text-muted-foreground">
                Conflicts resolved. A merge is in progress and needs a final merge commit to finish.
              </span>
              <Button onClick={completeMerge} disabled={completing} className="gap-1.5" size="sm">
                <Check size={12} /> {completing ? "Completing…" : "Finish in-progress merge"}
                <Kbd keys="ctrl+alt+enter" tone="onPrimary" className="ml-1" />
              </Button>
              <span className="text-[10px] text-muted-foreground/60">
                Creates a local merge commit. No push happens — your remote branch is unaffected.
              </span>
            </>
          ) : (
            <span className="text-[11px] text-muted-foreground">
              No conflicts — nothing to resolve here.
            </span>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {selectedPath && (
        <ConflictView
          key={selectedPath}
          sessionId={sessionId}
          path={selectedPath}
          fileCount={files.length}
          inZen={inZen}
          onToggleZen={onToggleZen}
        />
      )}
    </div>
  );
}


function ConflictView({
  sessionId,
  path,
  fileCount,
  inZen,
  onToggleZen,
}: {
  sessionId: string;
  path: string;
  fileCount: number;
  inZen?: boolean;
  onToggleZen?: () => void;
}) {
  const key = conflictKey(sessionId, path);
  const [stateMap, setStateMap] = useAtom(conflictStateMapAtom);
  const current = stateMap[key];
  const [intentMap, setIntentMap] = useAtom(conflictIntentMapAtom);
  const intent = intentMap[key] ?? "";
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const addToast = useSetAtom(toastsAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const [focusedRegion, setFocusedRegion] = useState<number>(0);
  const [collapsedRegions, setCollapsedRegions] = useState<Set<number>>(new Set());
  const [sending, setSending] = useState(false);
  const zenStateInner = useAtomValue(zenModeAtom);
  const conflictsZenInner = useAtomValue(conflictsZenOpenAtom);
  const listenerActiveInner = inZen
    ? conflictsZenInner
    : !(zenStateInner.open || conflictsZenInner);

  const toggleRegionCollapse = useCallback((idx: number) => {
    setCollapsedRegions((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }, []);

  useEffect(() => {
    if (current?.loaded) return;
    invoke<{ ours: string; theirs: string; merged: string }>("get_file_conflict_versions", { sessionId, path })
      .then((data) => {
        setStateMap((prev) => ({ ...prev, [key]: { ...data, originalMerged: data.merged, loaded: true } }));
      })
      .catch((e) => addToast({ message: "Load conflict failed", description: String(e), type: "error" }));
  }, [current, key, sessionId, path, setStateMap, addToast]);

  const regions = useMemo(() => current?.loaded ? parseRegions(current.merged) : [], [current?.merged, current?.loaded]);

  /// Line ranges (0-indexed, inclusive) to hide in the merged pane when their
  /// region is collapsed. CodeMirror's fold extension consumes these via
  /// dispatched effects in CodePane — Space then folds/unfolds the entire
  /// chunk, matching the diff-Zen behavior the user expects.
  const mergedFoldRanges = useMemo(() => {
    const out: { fromLine: number; toLine: number }[] = [];
    for (const idx of collapsedRegions) {
      const r = regions[idx];
      if (!r) continue;
      out.push({ fromLine: r.start, toLine: r.end });
    }
    return out;
  }, [collapsedRegions, regions]);

  useEffect(() => {
    if (focusedRegion >= regions.length) setFocusedRegion(Math.max(0, regions.length - 1));
  }, [regions.length, focusedRegion]);

  const updateMerged = useCallback((next: string) => {
    setStateMap((prev) => {
      const cs = prev[key] ?? { ours: "", theirs: "", merged: "", originalMerged: "", loaded: false };
      return { ...prev, [key]: { ...cs, merged: next } };
    });
  }, [key, setStateMap]);

  const applyRegion = useCallback((regionIdx: number, choice: "ours" | "theirs" | "both") => {
    if (!current) return;
    const regs = parseRegions(current.merged);
    const region = regs[regionIdx];
    if (!region) return;
    updateMerged(applyChoice(current.merged, region, choice));
  }, [current, updateMerged]);

  const resetMerged = useCallback(() => {
    if (!current) return;
    updateMerged(current.originalMerged);
    addToast({ message: "Reset", description: "Merged restored with conflict markers", type: "info" });
  }, [current, updateMerged, addToast]);

  const askClaude = useCallback(async () => {
    if (!current || sending) return;
    setSending(true);
    try {
      const prompt = await invoke<string>("build_conflict_prompt", {
        sessionId,
        path,
        ours: current.ours,
        theirs: current.theirs,
        originalMerged: current.originalMerged,
        intent: intent || null,
      });
      setFocusZone("terminal");
      terminalService.focusActive();
      await terminalService.writeToSession(sessionId, `${prompt}\r`);
      addToast({ message: "Sent to Claude", description: "Prompt submitted with conflict context", type: "success" });
      setIntentMap((prev) => ({ ...prev, [key]: "" }));
    } catch (e) {
      addToast({ message: "Failed", description: String(e), type: "error" });
    } finally {
      setSending(false);
    }
  }, [current, sending, sessionId, path, intent, key, setFocusZone, addToast, setIntentMap]);

  const saveResolution = useCallback(async () => {
    if (!current) return;
    // Guard: merged must not contain unresolved conflict markers.
    const hasMarkers = current.merged
      .split("\n")
      .some((line) => START_RE.test(line) || SEP_RE.test(line) || END_RE.test(line));
    if (hasMarkers) {
      addToast({
        message: "Cannot save",
        description: "Resolve all conflict markers (<<<<<<< / ======= / >>>>>>>) before saving.",
        type: "error",
      });
      return;
    }
    try {
      await invoke<string[]>("save_conflict_resolution", { sessionId, path, merged: current.merged });
      refreshConflicts(sessionId);
      addToast({ message: "Resolved", description: `${path} saved and staged`, type: "success" });
    } catch (e) {
      addToast({ message: "Save failed", description: String(e), type: "error" });
    }
  }, [current, sessionId, path, refreshConflicts, addToast]);

  useEffect(() => {
    if (!listenerActiveInner) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inEditor = target?.tagName === "TEXTAREA"
        || target?.tagName === "INPUT"
        || !!target?.closest(".cm-editor");
      if (!inEditor && regions.length > 0) {
        // Arrow or J/K navigation between regions (parity with DiffView hunk nav).
        if ((e.key === "ArrowDown" || e.key === "j" || e.key === "J") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          setFocusedRegion((i) => Math.min(i + 1, regions.length - 1));
          return;
        }
        if ((e.key === "ArrowUp" || e.key === "k" || e.key === "K") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          setFocusedRegion((i) => Math.max(i - 1, 0));
          return;
        }
        // Space: toggle collapse/expand on the focused region header (diff parity).
        if ((e.key === " " || e.code === "Space") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          toggleRegionCollapse(focusedRegion);
          return;
        }
        if (e.key === "o" || e.key === "O") { e.preventDefault(); applyRegion(focusedRegion, "ours"); return; }
        if (e.key === "t" || e.key === "T") { e.preventDefault(); applyRegion(focusedRegion, "theirs"); return; }
        if (e.key === "b" || e.key === "B") { e.preventDefault(); applyRegion(focusedRegion, "both"); return; }
      }
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey) return;
      if (e.code === "KeyO") { e.preventDefault(); applyRegion(focusedRegion, "ours"); }
      else if (e.code === "KeyT") { e.preventDefault(); applyRegion(focusedRegion, "theirs"); }
      else if (e.code === "KeyZ") { e.preventDefault(); resetMerged(); }
      else if (e.key === "Enter") { e.preventDefault(); saveResolution(); }
    }
    function onResolveActive() { askClaude(); }
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("cluihud:resolve-conflict-active-tab", onResolveActive);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("cluihud:resolve-conflict-active-tab", onResolveActive);
    };
  }, [regions, focusedRegion, applyRegion, resetMerged, saveResolution, askClaude, toggleRegionCollapse, listenerActiveInner]);

  if (!current?.loaded) {
    return <div className="flex flex-1 items-center justify-center text-[11px] text-muted-foreground">Loading conflict…</div>;
  }

  const isDirty = current.merged !== current.originalMerged;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Row 1: Info */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <AlertTriangle size={12} className="text-yellow-400" />
        <span className="text-[10px] font-medium uppercase tracking-wider text-yellow-400">Conflict</span>
        <span className="truncate font-mono text-[11px] text-foreground/85" title={path}>{path}</span>
        {fileCount > 1 && (
          <button
            onClick={() => document.dispatchEvent(new CustomEvent("cluihud:toggle-annotations-drawer"))}
            className="shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[9px] text-muted-foreground hover:text-foreground transition-colors"
            title="Toggle files drawer (Ctrl+Shift+J)"
          >
            +{fileCount - 1} more
          </button>
        )}
        <span className="ml-auto flex items-center gap-2">
          <span className="text-[10px] text-muted-foreground">{regions.length} region{regions.length !== 1 ? "s" : ""}</span>
          {isDirty && <span className="text-[10px] text-amber-400">modified</span>}
          {onToggleZen && (
            <button onClick={onToggleZen} className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors" title={inZen ? "Collapse (Ctrl+Shift+0)" : "Expand (Ctrl+Shift+0)"}>
              {inZen ? <Minimize2 size={11} /> : <Maximize2 size={11} />}
            </button>
          )}
        </span>
      </div>

      {/* Row 2: Chunk-header-style actions (diff panel visual parity) */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 bg-secondary/30 px-3 py-0.5">
        {regions.length > 0 ? (
          <>
            {/* @@ header cue — mirrors the diff panel chunk marker */}
            <span className="font-mono text-[10px] text-muted-foreground">
              @@ region {focusedRegion + 1}/{regions.length} @@
            </span>
            {regions[focusedRegion] && (
              <span className="font-mono text-[10px]">
                <span className="text-blue-400">-{regions[focusedRegion].oursLines.length}</span>
                <span className="text-muted-foreground/40 px-0.5">/</span>
                <span className="text-purple-400">+{regions[focusedRegion].theirsLines.length}</span>
              </span>
            )}
            <button onClick={() => applyRegion(focusedRegion, "ours")} className="ml-2 flex h-5 items-center gap-1 rounded bg-blue-500/15 px-2 text-[10px] text-blue-300 hover:bg-blue-500/25 transition-colors"><Minus size={9} /> Ours <Kbd keys="o" /></button>
            <button onClick={() => applyRegion(focusedRegion, "theirs")} className="flex h-5 items-center gap-1 rounded bg-purple-500/15 px-2 text-[10px] text-purple-300 hover:bg-purple-500/25 transition-colors"><Minus size={9} /> Theirs <Kbd keys="t" /></button>
            <button onClick={() => applyRegion(focusedRegion, "both")} className="flex h-5 items-center gap-1 rounded bg-green-500/15 px-2 text-[10px] text-green-300 hover:bg-green-500/25 transition-colors"><Minus size={9} /> Both <Kbd keys="b" /></button>
            <span className="ml-2 text-[9px] text-muted-foreground/60">↑↓/JK move · Space collapse</span>
          </>
        ) : (
          <span className="text-[10px] text-green-400">No markers remain — ready to save.</span>
        )}
        <div className="ml-auto flex items-center gap-1">
          {isDirty && (
            <Button variant="secondary" size="sm" onClick={resetMerged} className="h-6 gap-1 px-2 text-[10px]">
              <RotateCcw size={10} /> Reset <Kbd keys="ctrl+shift+z" />
            </Button>
          )}
          <Button size="sm" onClick={saveResolution} className="h-6 gap-1 px-2 text-[10px]">
            <Save size={10} /> Save <Kbd keys="ctrl+shift+enter" />
          </Button>
        </div>
      </div>

      {/* Row 3: Region headers — one row per conflict region in @@ chunk style.
          Click to focus; Chevron to collapse; inline line-count and action buttons. */}
      {regions.length > 0 && (
        <div className="flex shrink-0 flex-col border-b border-border/50 bg-card/30">
          {regions.map((region, i) => {
            const collapsed = collapsedRegions.has(i);
            const focused = i === focusedRegion;
            return (
              <div
                key={i}
                onClick={(e) => {
                  setFocusedRegion(i);
                  // Drop focus from the row so the next Space keypress is
                  // owned exclusively by the global keydown listener — without
                  // this, the browser's default button activation fires Space
                  // on the focused button AND the global handler runs, causing
                  // the region to toggle twice.
                  (e.currentTarget as HTMLElement).blur();
                }}
                className={`flex items-center gap-2 border-t border-border/20 px-3 py-0.5 cursor-pointer transition-colors ${
                  focused
                    ? "border-l-2 border-l-yellow-500 bg-yellow-500/10"
                    : "hover:bg-secondary/40"
                }`}
              >
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleRegionCollapse(i);
                    (e.currentTarget as HTMLElement).blur();
                  }}
                  className="flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary"
                  title={collapsed ? "Expand (Space)" : "Collapse (Space)"}
                >
                  <ChevronRight size={10} className={`transition-transform ${collapsed ? "" : "rotate-90"}`} />
                </button>
                <span className={`font-mono text-[10px] ${focused ? "text-yellow-300" : "text-muted-foreground"}`}>
                  @@ region {i + 1} @@
                </span>
                <span className="font-mono text-[10px]">
                  <span className="text-blue-400">-{region.oursLines.length}</span>
                  <span className="text-muted-foreground/40 px-0.5">/</span>
                  <span className="text-purple-400">+{region.theirsLines.length}</span>
                </span>
                {!collapsed && (
                  <span className="truncate text-[10px] text-muted-foreground/60">
                    {(region.oursLines[0] ?? region.theirsLines[0] ?? "").trim() || "(empty line)"}
                  </span>
                )}
                {collapsed && (
                  <span className="text-[10px] text-muted-foreground/50">
                    collapsed — {region.oursLines.length + region.theirsLines.length} lines hidden
                  </span>
                )}
                <span className="ml-auto flex shrink-0 items-center gap-0.5">
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "ours"); }} className="rounded bg-blue-500/10 px-1.5 py-0.5 text-[9px] text-blue-300 hover:bg-blue-500/25 transition-colors" title="Use Ours (O)">O</button>
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "theirs"); }} className="rounded bg-purple-500/10 px-1.5 py-0.5 text-[9px] text-purple-300 hover:bg-purple-500/25 transition-colors" title="Use Theirs (T)">T</button>
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "both"); }} className="rounded bg-green-500/10 px-1.5 py-0.5 text-[9px] text-green-300 hover:bg-green-500/25 transition-colors" title="Use Both (B)">B</button>
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* 3-pane */}
      <div className="grid min-h-0 flex-1 grid-cols-3 gap-0">
        <CodePane title="Ours" accent="text-blue-400" value={current.ours} readOnly filePath={path} wrap={inZen} />
        <CodePane title="Theirs" accent="text-purple-400" value={current.theirs} readOnly filePath={path} wrap={inZen} />
        <CodePane
          title="Merged (editable)"
          accent="text-green-400"
          value={current.merged}
          onChange={updateMerged}
          hint="O/T/B accept · ↑↓ region · space fold"
          filePath={path}
          wrap={inZen}
          foldLineRanges={mergedFoldRanges}
        />
      </div>

      {/* Intent + Ask Claude */}
      <div className="flex shrink-0 flex-col gap-1.5 border-t border-border/50 bg-card/40 px-3 py-2">
        <div className="flex items-center gap-2">
          <Sparkles size={10} className="text-muted-foreground" />
          <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Intent for Claude (optional)</label>
          <span className="ml-auto text-[9px] text-muted-foreground/60">Ctrl+Shift+R sends</span>
        </div>
        <textarea
          value={intent}
          onChange={(e) => setIntentMap((prev) => ({ ...prev, [key]: e.target.value }))}
          placeholder="e.g. Keep theirs for imports, merge both for the handler logic, drop the debug log…"
          className="h-14 resize-none rounded border border-border/50 bg-background px-2 py-1 text-[11px] outline-none focus:ring-1 focus:ring-orange-500/50"
          spellCheck={false}
        />
        <div className="flex justify-end">
          <Button size="sm" onClick={askClaude} disabled={sending} className="h-6 gap-1 px-2 text-[10px]">
            {sending ? "Sending…" : <><Send size={10} /> Ask Claude</>}
          </Button>
        </div>
      </div>
    </div>
  );
}

/// Code pane with CodeMirror (syntax highlighting + line numbers).
function CodePane({ title, accent, value, onChange, readOnly, hint, filePath, wrap = false, foldLineRanges }: {
  title: string;
  accent: string;
  value: string;
  onChange?: (v: string) => void;
  readOnly?: boolean;
  hint?: string;
  filePath: string;
  wrap?: boolean;
  /// Line ranges (0-indexed, inclusive) to fold in this editor. Empty/omitted
  /// = no folds applied. The pane reconciles the editor's current folds with
  /// this prop on every change, so the caller can drive folds purely as data.
  foldLineRanges?: { fromLine: number; toLine: number }[];
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const editableCompartment = useRef(new Compartment());

  // Initial mount: create the EditorView once per (filePath, readOnly, wrap).
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (viewRef.current) { viewRef.current.destroy(); viewRef.current = null; }
    container.replaceChildren();

    const extensions = [
      basicSetup,
      cmTheme,
      syntaxHighlighting(oneDarkHighlightStyle),
      getLanguageExtension(filePath),
      codeFolding(),
      editableCompartment.current.of(EditorView.editable.of(!readOnly)),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && onChange) {
          onChange(update.state.doc.toString());
        }
      }),
    ];
    if (wrap) extensions.push(EditorView.lineWrapping);

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({ state, parent: container });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
      container.replaceChildren();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath, readOnly, wrap]);

  // Sync external value changes into the editor without losing cursor.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  // Reconcile fold state with the foldLineRanges prop. We unfold everything
  // currently folded then fold the desired ranges — simple and idempotent,
  // and the regions are tiny (one per conflict), so the cost is negligible.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const ranges = foldLineRanges ?? [];
    const doc = view.state.doc;
    const desired: { from: number; to: number }[] = [];
    for (const { fromLine, toLine } of ranges) {
      if (fromLine < 0 || fromLine >= doc.lines) continue;
      const safeTo = Math.min(toLine, doc.lines - 1);
      if (safeTo < fromLine) continue;
      desired.push({
        from: doc.line(fromLine + 1).from,
        to: doc.line(safeTo + 1).to,
      });
    }
    const effects: StateEffect<unknown>[] = [];
    foldedRanges(view.state).between(0, doc.length, (from, to) => {
      effects.push(unfoldEffect.of({ from, to }));
    });
    for (const r of desired) effects.push(foldEffect.of(r));
    if (effects.length > 0) view.dispatch({ effects });
  }, [foldLineRanges, value]);

  return (
    <div className="flex min-h-0 flex-col border-r border-border/50 last:border-r-0">
      <div className="flex shrink-0 items-center gap-1 border-b border-border/30 px-2 py-1">
        <Check size={10} className={accent} />
        <span className={`text-[10px] font-medium uppercase tracking-wider ${accent}`}>{title}</span>
        {hint && <span className="ml-auto text-[9px] text-muted-foreground/60">{hint}</span>}
      </div>
      <div ref={containerRef} className="min-h-0 flex-1 overflow-hidden" />
    </div>
  );
}
