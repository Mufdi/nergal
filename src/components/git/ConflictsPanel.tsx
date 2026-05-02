import { useEffect, useCallback, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { EditorView, Decoration, WidgetType, type DecorationSet } from "@codemirror/view";
import { EditorState, Compartment, RangeSetBuilder } from "@codemirror/state";
import { basicSetup } from "codemirror";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import {
  syntaxHighlighting,
  codeFolding,
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
import { zenModeAtom, prZenAtom } from "@/stores/zenMode";
import * as terminalService from "@/components/terminal/terminalService";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import {
  AlertTriangle,
  Check,
  ChevronLeft as ChevronLeftIcon,
  ChevronRight as ChevronRightIcon,
  Sparkles,
  Save,
  RotateCcw,
  Send,
  FileCheck,
  Maximize2,
  Minimize2,
  Link2,
  Unlink2,
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
  // IntelliJ-style conflict line tints. Cool blue (ours) vs warm rose (theirs)
  // gives clear hue separation on a dark canvas — the previous blue/purple
  // pairing was too close on the wheel and washed out at low alpha. Stronger
  // unfocused alpha (0.18 vs 0.10) so the bands read at a glance; focused
  // ramps further (0.32) so the active region pops without strobing.
  ".cluihud-conflict-line.tone-ours": { backgroundColor: "rgba(59, 130, 246, 0.18)" },
  ".cluihud-conflict-line.tone-ours.is-focused": { backgroundColor: "rgba(59, 130, 246, 0.32)" },
  ".cluihud-conflict-line.tone-theirs": { backgroundColor: "rgba(244, 63, 94, 0.18)" },
  ".cluihud-conflict-line.tone-theirs.is-focused": { backgroundColor: "rgba(244, 63, 94, 0.32)" },
  ".cluihud-conflict-line.tone-marker": { backgroundColor: "rgba(245, 158, 11, 0.14)", color: "rgba(245, 158, 11, 0.95)" },
  ".cluihud-conflict-line.tone-marker.is-focused": { backgroundColor: "rgba(245, 158, 11, 0.26)" },
  // Inline action bar widget — block decoration sitting above each <<<<<<<
  // line. The button row gives one-click accept without leaving the editor.
  ".cluihud-accept-bar": {
    display: "flex", alignItems: "center", gap: "6px",
    padding: "3px 8px",
    borderTop: "1px solid rgba(245, 158, 11, 0.25)",
    borderBottom: "1px solid rgba(245, 158, 11, 0.25)",
    backgroundColor: "rgba(245, 158, 11, 0.06)",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: "10px",
  },
  ".cluihud-accept-bar.is-focused": { backgroundColor: "rgba(245, 158, 11, 0.14)" },
  ".cluihud-accept-bar .label": { color: "rgba(245, 158, 11, 0.9)", marginRight: "auto" },
  ".cluihud-accept-bar button": {
    padding: "1px 7px", borderRadius: "3px", border: "none", cursor: "pointer",
    fontFamily: "inherit", fontSize: "10px", fontWeight: "500",
  },
  ".cluihud-accept-bar .btn-ours": { backgroundColor: "rgba(59, 130, 246, 0.20)", color: "#93c5fd" },
  ".cluihud-accept-bar .btn-ours:hover": { backgroundColor: "rgba(59, 130, 246, 0.35)" },
  ".cluihud-accept-bar .btn-theirs": { backgroundColor: "rgba(244, 63, 94, 0.22)", color: "#fda4af" },
  ".cluihud-accept-bar .btn-theirs:hover": { backgroundColor: "rgba(244, 63, 94, 0.40)" },
  ".cluihud-accept-bar .btn-both": { backgroundColor: "rgba(34, 197, 94, 0.20)", color: "#86efac" },
  ".cluihud-accept-bar .btn-both:hover": { backgroundColor: "rgba(34, 197, 94, 0.35)" },
}, { dark: true });

/// Block widget rendered above each `<<<<<<<` line in the merged pane. The
/// three buttons fire `onAccept(regionIdx, choice)` so the user can resolve
/// without leaving the editor — same operation as the row-list buttons but
/// anchored to the conflict it acts on. We avoid React inside the widget
/// (CodeMirror owns the lifecycle of widget DOM) and keep the buttons plain
/// HTML with inline event listeners.
class AcceptActionsWidget extends WidgetType {
  constructor(
    private readonly regionIdx: number,
    private readonly focused: boolean,
    private readonly oursCount: number,
    private readonly theirsCount: number,
    private readonly onAccept: (idx: number, choice: "ours" | "theirs" | "both") => void,
  ) { super(); }

  eq(other: AcceptActionsWidget): boolean {
    return this.regionIdx === other.regionIdx
      && this.focused === other.focused
      && this.oursCount === other.oursCount
      && this.theirsCount === other.theirsCount;
  }

  toDOM(): HTMLElement {
    const root = document.createElement("div");
    root.className = `cluihud-accept-bar${this.focused ? " is-focused" : ""}`;
    const label = document.createElement("span");
    label.className = "label";
    label.textContent = `region ${this.regionIdx + 1} · -${this.oursCount} / +${this.theirsCount}`;
    root.appendChild(label);
    const make = (text: string, cls: string, choice: "ours" | "theirs" | "both") => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = cls;
      btn.textContent = text;
      btn.onmousedown = (e) => e.preventDefault();
      btn.onclick = (e) => { e.stopPropagation(); this.onAccept(this.regionIdx, choice); };
      return btn;
    };
    root.appendChild(make("Ours (O)", "btn-ours", "ours"));
    root.appendChild(make("Theirs (T)", "btn-theirs", "theirs"));
    root.appendChild(make("Both (B)", "btn-both", "both"));
    return root;
  }

  ignoreEvent(): boolean { return false; }
}

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
  const prZen = useAtomValue(prZenAtom);
  // Conflicts has its own Zen overlay (single-pane, no sidebar). Listener
  // gating: in-Zen instance fires when its overlay is open; the inline chip
  // copy fires only when no Zen is up. Diff/PR Zen also block the inline copy.
  const listenerActive = inZen
    ? conflictsZen
    : !(zenState.open || conflictsZen || prZen !== null);

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

  const handleNavFile = useCallback((direction: "prev" | "next") => {
    if (files.length < 2) return;
    const idx = selectedPath ? files.indexOf(selectedPath) : -1;
    if (idx === -1) return;
    const nextIdx = direction === "next"
      ? (idx + 1) % files.length
      : (idx - 1 + files.length) % files.length;
    const next = files[nextIdx];
    if (next && next !== selectedPath) {
      setSelectedMap((prev) => ({ ...prev, [sessionId]: next }));
    }
  }, [files, selectedPath, sessionId, setSelectedMap]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {selectedPath && (
        <ConflictView
          key={selectedPath}
          sessionId={sessionId}
          path={selectedPath}
          files={files}
          inZen={inZen}
          onToggleZen={onToggleZen}
          onNavFile={files.length > 1 ? handleNavFile : undefined}
          onPickFile={(p) => setSelectedMap((prev) => ({ ...prev, [sessionId]: p }))}
        />
      )}
    </div>
  );
}


function ConflictView({
  sessionId,
  path,
  files,
  inZen,
  onToggleZen,
  onNavFile,
  onPickFile,
}: {
  sessionId: string;
  path: string;
  files: string[];
  inZen?: boolean;
  onToggleZen?: () => void;
  onNavFile?: (direction: "prev" | "next") => void;
  /// Commit a specific file from the picker. ConflictsPanel wraps it with
  /// `setSelectedMap` so picking causes the parent to re-key ConflictView
  /// with the new path.
  onPickFile?: (path: string) => void;
}) {
  const fileCount = files.length;
  const key = conflictKey(sessionId, path);
  const [stateMap, setStateMap] = useAtom(conflictStateMapAtom);
  const current = stateMap[key];
  const [intentMap, setIntentMap] = useAtom(conflictIntentMapAtom);
  const intent = intentMap[key] ?? "";
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const addToast = useSetAtom(toastsAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const [focusedRegion, setFocusedRegion] = useState<number>(0);
  const [sending, setSending] = useState(false);
  /// File picker state — mirrors PrViewer's pattern. The picker swaps which
  /// file the panel shows; opened via Ctrl+Shift+K (global) or the chevrons.
  /// j/k drives the cursor without committing; Enter commits + closes; Esc
  /// closes without changing the file. Cursor seeds from the active file
  /// every time the picker opens so navigation starts in a useful spot.
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerCursor, setPickerCursor] = useState(0);
  /// Sync-scroll lock. On by default — JetBrains-style 3-pane review only
  /// makes sense when scrolling one pane drags the others. The toggle exists
  /// for the rare moment the user wants to look at theirs at line 200 while
  /// keeping merged at line 50.
  const [syncScroll, setSyncScroll] = useState(true);
  // Views as state (not ref) so ConnectorStrip — which can't observe ref
  // mutations — re-renders when each pane's EditorView mounts. The sync
  // listener and click-to-scroll effects also read these to attach handlers.
  const [oursView, setOursView] = useState<EditorView | null>(null);
  const [mergedView, setMergedView] = useState<EditorView | null>(null);
  const [theirsView, setTheirsView] = useState<EditorView | null>(null);
  /// Per-pane "ignore scroll listener until ts" map. When the click-to-region
  /// flow dispatches scrollIntoView on all three panes, those programmatic
  /// scrolls fire DOM scroll events that would feed back into the sync
  /// listener and bounce the panes around (the merged↔ours mapping is
  /// asymmetric inside conflict regions, so a roundtrip jumps to a different
  /// line). We bump these timestamps to suppress the listeners briefly, both
  /// on focus-region jumps and after each programmatic syncSet.
  const ignoreUntilRef = useRef<{ ours: number; merged: number; theirs: number }>({ ours: 0, merged: 0, theirs: 0 });
  const zenStateInner = useAtomValue(zenModeAtom);
  const conflictsZenInner = useAtomValue(conflictsZenOpenAtom);
  const prZenInner = useAtomValue(prZenAtom);
  const listenerActiveInner = inZen
    ? conflictsZenInner
    : !(zenStateInner.open || conflictsZenInner || prZenInner !== null);

  useEffect(() => {
    if (current?.loaded) return;
    invoke<{ ours: string; theirs: string; merged: string }>("get_file_conflict_versions", { sessionId, path })
      .then((data) => {
        setStateMap((prev) => ({ ...prev, [key]: { ...data, originalMerged: data.merged, loaded: true } }));
      })
      .catch((e) => addToast({ message: "Load conflict failed", description: String(e), type: "error" }));
  }, [current, key, sessionId, path, setStateMap, addToast]);

  const regions = useMemo(() => current?.loaded ? parseRegions(current.merged) : [], [current?.merged, current?.loaded]);

  /// Per-region line offsets in the ours and theirs reference files. The
  /// merged document inserts conflict markers and theirs content (or ours
  /// content for the theirs file), so a region at merged line N maps to a
  /// different line in each side. We walk regions in order tracking how
  /// many lines have been "added" by markers/other-side content; the result
  /// is what the side panes need to scroll to and highlight when the user
  /// focuses region i. Without this, the side panes would stay stuck on
  /// line 0 while the merged pane navigates — defeating the 3-pane purpose.
  const sideMapping = useMemo(() => {
    const out: { oursLine: number; theirsLine: number; oursLen: number; theirsLen: number }[] = [];
    let oursLine = 0;
    let theirsLine = 0;
    let mergedCursor = 0;
    for (const r of regions) {
      const sharedCount = r.start - mergedCursor;
      oursLine += sharedCount;
      theirsLine += sharedCount;
      out.push({
        oursLine,
        theirsLine,
        oursLen: r.oursLines.length,
        theirsLen: r.theirsLines.length,
      });
      oursLine += r.oursLines.length;
      theirsLine += r.theirsLines.length;
      mergedCursor = r.end + 1;
    }
    return out;
  }, [regions]);

  /// IntelliJ-style inline highlighting for the merged pane. Each region
  /// gets three tint bands: blue on the ours-content lines, rose on the
  /// theirs-content lines, amber on the three marker lines (<<<<<<<, =======,
  /// >>>>>>>). Focused region renders brighter so j/k navigation lights up
  /// the active chunk without losing context of the others.
  const mergedHighlights = useMemo<ConflictHighlight[]>(() => {
    const out: ConflictHighlight[] = [];
    regions.forEach((r, idx) => {
      const focused = idx === focusedRegion;
      out.push({ fromLine: r.start, toLine: r.start, tone: "marker", focused });
      if (r.sep > r.start + 1) {
        out.push({ fromLine: r.start + 1, toLine: r.sep - 1, tone: "ours", focused });
      }
      out.push({ fromLine: r.sep, toLine: r.sep, tone: "marker", focused });
      if (r.end > r.sep + 1) {
        out.push({ fromLine: r.sep + 1, toLine: r.end - 1, tone: "theirs", focused });
      }
      out.push({ fromLine: r.end, toLine: r.end, tone: "marker", focused });
    });
    return out;
  }, [regions, focusedRegion]);

  /// Block-widget action bars rendered above each `<<<<<<<` line in the
  /// merged pane. Wires the buttons to applyRegion via onAccept so accepting
  /// a chunk inline matches Ctrl-driven row-list buttons line-for-line.
  const mergedInlineActions = useMemo<ConflictInlineAction[]>(() => {
    return regions.map((r, idx) => ({
      atLine: r.start,
      regionIdx: idx,
      focused: idx === focusedRegion,
      oursCount: r.oursLines.length,
      theirsCount: r.theirsLines.length,
    }));
  }, [regions, focusedRegion]);

  /// IntelliJ-style highlights for the side reference panes. Each region's
  /// chunk-of-interest gets a tinted band so the user can spot the conflicting
  /// region in the full ours/theirs files without scanning. Empty chunks
  /// (one side added nothing) get skipped — there's no line range to paint.
  const oursHighlights = useMemo<ConflictHighlight[]>(() => {
    const out: ConflictHighlight[] = [];
    sideMapping.forEach((m, idx) => {
      if (m.oursLen <= 0) return;
      out.push({
        fromLine: m.oursLine,
        toLine: m.oursLine + m.oursLen - 1,
        tone: "ours",
        focused: idx === focusedRegion,
      });
    });
    return out;
  }, [sideMapping, focusedRegion]);

  const theirsHighlights = useMemo<ConflictHighlight[]>(() => {
    const out: ConflictHighlight[] = [];
    sideMapping.forEach((m, idx) => {
      if (m.theirsLen <= 0) return;
      out.push({
        fromLine: m.theirsLine,
        toLine: m.theirsLine + m.theirsLen - 1,
        tone: "theirs",
        focused: idx === focusedRegion,
      });
    });
    return out;
  }, [sideMapping, focusedRegion]);

  /// Connector specs for the SVG strips between panes. Each region produces
  /// at most one ours-side and one theirs-side trapezoid:
  /// - ours strip: ours pane chunk lines → merged pane's ours-content lines
  ///   (the body between `<<<<<<<` and `=======`)
  /// - theirs strip: merged pane's theirs-content lines → theirs pane chunk
  ///   lines (the body between `=======` and `>>>>>>>`)
  /// Empty sides (one branch added nothing) skip — a zero-height polygon
  /// would just be a line segment that conveys no information.
  const oursConnectorSpecs = useMemo<ConnectorSpec[]>(() => {
    const out: ConnectorSpec[] = [];
    regions.forEach((r, idx) => {
      const m = sideMapping[idx];
      if (!m || m.oursLen === 0) return;
      out.push({
        leftFrom: m.oursLine,
        leftTo: m.oursLine + m.oursLen - 1,
        rightFrom: r.start + 1,
        rightTo: r.sep - 1,
        focused: idx === focusedRegion,
        regionIdx: idx,
      });
    });
    return out;
  }, [regions, sideMapping, focusedRegion]);

  const theirsConnectorSpecs = useMemo<ConnectorSpec[]>(() => {
    const out: ConnectorSpec[] = [];
    regions.forEach((r, idx) => {
      const m = sideMapping[idx];
      if (!m || m.theirsLen === 0) return;
      out.push({
        leftFrom: r.sep + 1,
        leftTo: r.end - 1,
        rightFrom: m.theirsLine,
        rightTo: m.theirsLine + m.theirsLen - 1,
        focused: idx === focusedRegion,
        regionIdx: idx,
      });
    });
    return out;
  }, [regions, sideMapping, focusedRegion]);

  /// When the user navigates regions (via j/k, header-row click, or chevron
  /// nav), drag all three panes to the corresponding region. The nonce bumps
  /// on every focusedRegion change so consecutive picks of the same region
  /// re-scroll if the user moved the editor manually. Side panes use the
  /// sideMapping offsets; the merged pane uses the marker line directly.
  const [scrollNonce, setScrollNonce] = useState(0);
  useEffect(() => {
    setScrollNonce((n) => n + 1);
  }, [focusedRegion]);
  const mergedScrollTarget = useMemo(() => {
    const region = regions[focusedRegion];
    if (!region) return null;
    return { line: region.start, nonce: scrollNonce };
  }, [regions, focusedRegion, scrollNonce]);
  const oursScrollTarget = useMemo(() => {
    const m = sideMapping[focusedRegion];
    if (!m) return null;
    return { line: m.oursLine, nonce: scrollNonce };
  }, [sideMapping, focusedRegion, scrollNonce]);
  const theirsScrollTarget = useMemo(() => {
    const m = sideMapping[focusedRegion];
    if (!m) return null;
    return { line: m.theirsLine, nonce: scrollNonce };
  }, [sideMapping, focusedRegion, scrollNonce]);

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

  /// Accept the same choice for every remaining conflict region. Iterates
  /// bottom-up so each splice doesn't shift indices we haven't visited yet.
  /// Used by the "All Ours" / "All Theirs" buttons and Ctrl+Shift+O/T — handy
  /// when a merge is overwhelmingly dominated by one side and the user wants
  /// to bulk-resolve, then hand-edit a few exceptions in the merged pane.
  const acceptAll = useCallback((choice: "ours" | "theirs" | "both") => {
    if (!current) return;
    let next = current.merged;
    const regs = parseRegions(next);
    if (regs.length === 0) return;
    for (let i = regs.length - 1; i >= 0; i--) {
      next = applyChoice(next, regs[i], choice);
    }
    updateMerged(next);
    addToast({
      message: `Accepted all ${choice}`,
      description: `${regs.length} region${regs.length !== 1 ? "s" : ""} resolved. Edit the merged pane to refine.`,
      type: "success",
    });
  }, [current, updateMerged, addToast]);

  /// Piecewise line-mapping between the three panes. Lines outside any
  /// conflict region map 1:1 with a fixed drift (the merged view inserts 3
  /// markers + the other side's content per region). Lines inside a region
  /// have no clean mapping — we anchor them to the region's start in the
  /// target pane so sync-scroll never produces wildly out-of-sync views.
  const lineMaps = useMemo(() => {
    function mergedToOurs(line: number) {
      let drift = 0;
      for (let i = 0; i < regions.length; i++) {
        const r = regions[i];
        if (line < r.start) return line - drift;
        if (line <= r.end) return sideMapping[i].oursLine;
        drift += 3 + r.theirsLines.length;
      }
      return line - drift;
    }
    function mergedToTheirs(line: number) {
      let drift = 0;
      for (let i = 0; i < regions.length; i++) {
        const r = regions[i];
        if (line < r.start) return line - drift;
        if (line <= r.end) return sideMapping[i].theirsLine;
        drift += 3 + r.oursLines.length;
      }
      return line - drift;
    }
    function oursToMerged(line: number) {
      let drift = 0;
      for (let i = 0; i < regions.length; i++) {
        const m = sideMapping[i];
        if (line < m.oursLine) return line + drift;
        if (line < m.oursLine + m.oursLen) return regions[i].start + 1 + (line - m.oursLine);
        drift += 3 + regions[i].theirsLines.length;
      }
      return line + drift;
    }
    function theirsToMerged(line: number) {
      let drift = 0;
      for (let i = 0; i < regions.length; i++) {
        const m = sideMapping[i];
        if (line < m.theirsLine) return line + drift;
        if (line < m.theirsLine + m.theirsLen) return regions[i].sep + 1 + (line - m.theirsLine);
        drift += 3 + regions[i].oursLines.length;
      }
      return line + drift;
    }
    return { mergedToOurs, mergedToTheirs, oursToMerged, theirsToMerged };
  }, [regions, sideMapping]);

  /// When the user picks a region (scrollNonce bumps), each CodePane's own
  /// scrollToLine effect dispatches scrollIntoView. Those dispatched scrolls
  /// fire DOM events 1–2 frames later — and the sync listener would catch
  /// them with stale viewport data, snapping the panes to the wrong place.
  /// Suppressing all three listeners for ~250ms lets the dispatched scrolls
  /// finish without recursion. The ref is read inside the sync listener.
  useEffect(() => {
    if (scrollNonce === 0) return;
    const t = performance.now() + 250;
    ignoreUntilRef.current = { ours: t, merged: t, theirs: t };
  }, [scrollNonce]);

  /// Three-way scroll sync. Each pane's scroller fires a listener that
  /// translates its visible top line to the other panes via lineMaps and
  /// snaps their scrollTop accordingly. We use `lineBlockAtHeight(scrollTop)`
  /// to read the actually-visible top — `viewport.from` includes CodeMirror's
  /// off-screen render buffer and would yield a line above what the user
  /// sees. After each sync, we bump the destination panes' ignoreUntil so
  /// the cascade of programmatic scroll events they fire bails out instead
  /// of bouncing back and overwriting the source's position.
  useEffect(() => {
    if (!syncScroll) return;
    const ours = oursView;
    const merged = mergedView;
    const theirs = theirsView;
    if (!ours || !merged || !theirs) return;

    function topLineOf(view: EditorView): number {
      const top = view.scrollDOM.scrollTop;
      try {
        const block = view.lineBlockAtHeight(top);
        return view.state.doc.lineAt(block.from).number - 1;
      } catch {
        return 0;
      }
    }
    function syncSet(view: EditorView, line: number, paneName: "ours" | "merged" | "theirs") {
      const doc = view.state.doc;
      const lineIdx = Math.max(0, Math.min(doc.lines - 1, line));
      try {
        const pos = doc.line(lineIdx + 1).from;
        const block = view.lineBlockAt(pos);
        const target = block.top;
        // Skip if effectively already there — saves a scroll event we'd have
        // to suppress, and avoids pixel-level jitter from rounding.
        if (Math.abs(view.scrollDOM.scrollTop - target) < 1) return;
        // Suppress the destination pane's listener until the resulting scroll
        // event drains (one rAF frame is reliable; 100ms covers slower paths).
        ignoreUntilRef.current[paneName] = performance.now() + 100;
        view.scrollDOM.scrollTop = target;
      } catch {
        // line block may not exist if the view hasn't measured yet — ignore
      }
    }
    function makeListener(srcPane: "ours" | "merged" | "theirs") {
      return () => {
        if (performance.now() < ignoreUntilRef.current[srcPane]) return;
        if (srcPane === "merged") {
          const top = topLineOf(merged!);
          syncSet(ours!, lineMaps.mergedToOurs(top), "ours");
          syncSet(theirs!, lineMaps.mergedToTheirs(top), "theirs");
        } else if (srcPane === "ours") {
          const top = topLineOf(ours!);
          const m = lineMaps.oursToMerged(top);
          syncSet(merged!, m, "merged");
          syncSet(theirs!, lineMaps.mergedToTheirs(m), "theirs");
        } else if (srcPane === "theirs") {
          const top = topLineOf(theirs!);
          const m = lineMaps.theirsToMerged(top);
          syncSet(merged!, m, "merged");
          syncSet(ours!, lineMaps.mergedToOurs(m), "ours");
        }
      };
    }
    const oursListener = makeListener("ours");
    const mergedListener = makeListener("merged");
    const theirsListener = makeListener("theirs");
    ours.scrollDOM.addEventListener("scroll", oursListener, { passive: true });
    merged.scrollDOM.addEventListener("scroll", mergedListener, { passive: true });
    theirs.scrollDOM.addEventListener("scroll", theirsListener, { passive: true });
    return () => {
      ours.scrollDOM.removeEventListener("scroll", oursListener);
      merged.scrollDOM.removeEventListener("scroll", mergedListener);
      theirs.scrollDOM.removeEventListener("scroll", theirsListener);
    };
  }, [syncScroll, lineMaps, oursView, mergedView, theirsView]);

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

  /// Global Ctrl+Shift+K → toggle file picker. Same dispatcher PrViewer uses,
  /// so muscle memory carries between panels. Opening the picker seeds the
  /// cursor at the active file's index so j/k starts in context.
  useEffect(() => {
    if (!listenerActiveInner) return;
    function onToggle() {
      setPickerOpen((open) => {
        if (!open) {
          const idx = files.indexOf(path);
          setPickerCursor(idx >= 0 ? idx : 0);
        }
        return !open;
      });
    }
    document.addEventListener("cluihud:toggle-file-picker", onToggle);
    return () => document.removeEventListener("cluihud:toggle-file-picker", onToggle);
  }, [listenerActiveInner, files, path]);

  useEffect(() => {
    if (!listenerActiveInner) return;
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
          setPickerCursor((i) => (i + 1) % files.length);
          return;
        }
        if (e.code === "KeyK" || e.code === "ArrowUp") {
          e.preventDefault();
          e.stopPropagation();
          setPickerCursor((i) => (i - 1 + files.length) % files.length);
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
          setFocusedRegion((i) => Math.min(i + 1, regions.length - 1));
          return;
        }
        if ((e.key === "ArrowUp" || e.key === "k" || e.key === "K") && !(e.ctrlKey || e.metaKey) && !e.shiftKey) {
          e.preventDefault();
          setFocusedRegion((i) => Math.max(i - 1, 0));
          return;
        }
        if (e.key === "o" || e.key === "O") { e.preventDefault(); applyRegion(focusedRegion, "ours"); return; }
        if (e.key === "t" || e.key === "T") { e.preventDefault(); applyRegion(focusedRegion, "theirs"); return; }
        if (e.key === "b" || e.key === "B") { e.preventDefault(); applyRegion(focusedRegion, "both"); return; }
        if (e.key === "s" || e.key === "S") { e.preventDefault(); setSyncScroll((v) => !v); return; }
      }
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey) return;
      // Ctrl+Shift+O / Ctrl+Shift+T: accept ALL regions of one side. Mirrors
      // IntelliJ's "Apply Non-Conflicting Changes from Left/Right Side" but
      // applied to every conflict, since git's <<<<<<< blocks are by
      // definition conflicting. Lowercase O/T already handle per-region.
      if (e.code === "KeyO") { e.preventDefault(); acceptAll("ours"); }
      else if (e.code === "KeyT") { e.preventDefault(); acceptAll("theirs"); }
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
  }, [regions, focusedRegion, applyRegion, acceptAll, resetMerged, saveResolution, askClaude, listenerActiveInner, onNavFile, pickerOpen, pickerCursor, files, onPickFile]);

  if (!current?.loaded) {
    return <div className="flex flex-1 items-center justify-center text-[11px] text-muted-foreground">Loading conflict…</div>;
  }

  const isDirty = current.merged !== current.originalMerged;

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {/* Row 1: Info */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <AlertTriangle size={12} className="text-yellow-400" />
        <span className="text-[10px] font-medium uppercase tracking-wider text-yellow-400">Conflict</span>
        {onNavFile && (
          <span className="flex shrink-0 items-center">
            <button
              onClick={() => onNavFile("prev")}
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              aria-label="Previous file"
              title="Previous file (Ctrl+←)"
            >
              <ChevronLeftIcon size={11} />
            </button>
            <button
              onClick={() => onNavFile("next")}
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              aria-label="Next file"
              title="Next file (Ctrl+→)"
            >
              <ChevronRightIcon size={11} />
            </button>
          </span>
        )}
        <span className="truncate font-mono text-[11px] text-foreground/85" title={path}>{path}</span>
        {fileCount > 1 && (
          <span className="shrink-0 text-[9px] text-muted-foreground/70 tabular-nums">
            {files.indexOf(path) + 1}/{fileCount}
          </span>
        )}
        <span className="ml-auto flex items-center gap-2">
          <span className="text-[10px] text-muted-foreground">{regions.length} region{regions.length !== 1 ? "s" : ""}</span>
          {isDirty && <span className="text-[10px] text-amber-400">modified</span>}
          <button
            onClick={() => setSyncScroll((s) => !s)}
            className={`flex size-5 items-center justify-center rounded transition-colors ${
              syncScroll
                ? "text-orange-400 hover:bg-secondary"
                : "text-muted-foreground/60 hover:bg-secondary hover:text-foreground"
            }`}
            title={syncScroll ? "Scroll sync ON — click or press S to scroll panes independently" : "Scroll sync OFF — click or press S to re-sync the 3 panes"}
            aria-label={syncScroll ? "Disable scroll sync" : "Enable scroll sync"}
          >
            {syncScroll ? <Link2 size={11} /> : <Unlink2 size={11} />}
          </button>
          {onToggleZen && (
            <button onClick={onToggleZen} className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors" title={inZen ? "Collapse (Ctrl+Shift+0)" : "Expand (Ctrl+Shift+0)"}>
              {inZen ? <Minimize2 size={11} /> : <Maximize2 size={11} />}
            </button>
          )}
        </span>
      </div>

      {/* Row 2: Chunk-header-style actions (diff panel visual parity) — main
          row holds region info + accept buttons + Save; the keyboard hint
          lives on a thin sub-row so the bar doesn't double-stack. */}
      <div className="shrink-0 border-b border-border/50 bg-secondary/30">
        <div className="flex items-center gap-2 px-3 py-0.5">
          {regions.length > 0 ? (
            <>
              <span className="font-mono text-[10px] text-muted-foreground">
                @@ region {focusedRegion + 1}/{regions.length} @@
              </span>
              {regions[focusedRegion] && (
                <span className="font-mono text-[10px]">
                  <span className="text-blue-400">-{regions[focusedRegion].oursLines.length}</span>
                  <span className="text-muted-foreground/40 px-0.5">/</span>
                  <span className="text-rose-400">+{regions[focusedRegion].theirsLines.length}</span>
                </span>
              )}
              <button onClick={() => applyRegion(focusedRegion, "ours")} className="ml-2 flex h-5 items-center gap-1 rounded bg-blue-500/15 px-2 text-[10px] text-blue-300 hover:bg-blue-500/25 transition-colors" title="Accept ours for this region (O)">Ours <Kbd keys="o" /></button>
              <button onClick={() => applyRegion(focusedRegion, "theirs")} className="flex h-5 items-center gap-1 rounded bg-rose-500/15 px-2 text-[10px] text-rose-300 hover:bg-rose-500/25 transition-colors" title="Accept theirs for this region (T)">Theirs <Kbd keys="t" /></button>
              <button onClick={() => applyRegion(focusedRegion, "both")} className="flex h-5 items-center gap-1 rounded bg-green-500/15 px-2 text-[10px] text-green-300 hover:bg-green-500/25 transition-colors" title="Keep both versions stacked (ours then theirs). Useful when both sides added independent additions; you'll usually need to hand-edit afterward (B)">Both <Kbd keys="b" /></button>
              <span className="mx-1 h-3 w-px bg-border/50" />
              <button onClick={() => acceptAll("ours")} className="flex h-5 items-center whitespace-nowrap rounded border border-blue-500/40 bg-blue-500/5 px-2 text-[10px] text-blue-300 hover:bg-blue-500/20 transition-colors" title="Accept ours for ALL regions in this file (Ctrl+Shift+O)">All Ours</button>
              <button onClick={() => acceptAll("theirs")} className="flex h-5 items-center whitespace-nowrap rounded border border-rose-500/40 bg-rose-500/5 px-2 text-[10px] text-rose-300 hover:bg-rose-500/20 transition-colors" title="Accept theirs for ALL regions in this file (Ctrl+Shift+T)">All Theirs</button>
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
            <Button size="sm" onClick={saveResolution} className="h-6 gap-1 px-2 text-[10px]" title="Save resolution and stage the file (Ctrl+Shift+Enter)">
              <Save size={10} /> Save
            </Button>
          </div>
        </div>
        {regions.length > 0 && (
          <div className="border-t border-border/30 px-3 py-0.5 text-[9px] text-muted-foreground/60">
            ↑↓ / j k region · o / t / b region · ctrl+shift+o / t all · s sync · ctrl+shift+k picker · ctrl+shift+enter save · ctrl+shift+r ask Claude
          </div>
        )}
      </div>

      {/* Row 3: Region index — one row per conflict region. Pure navigation
          surface: click to focus, which sync-scrolls all three panes to the
          chunk and brightens the inline highlights. No collapse here — the
          panes are reference material; folding them out defeats the point. */}
      {regions.length > 0 && (
        <div className="flex shrink-0 flex-col border-b border-border/50 bg-card/30">
          {regions.map((region, i) => {
            const focused = i === focusedRegion;
            return (
              <div
                key={i}
                onClick={(e) => {
                  setFocusedRegion(i);
                  (e.currentTarget as HTMLElement).blur();
                }}
                className={`flex items-center gap-2 border-t border-border/20 px-3 py-0.5 cursor-pointer transition-colors ${
                  focused
                    ? "border-l-2 border-l-yellow-500 bg-yellow-500/10"
                    : "hover:bg-secondary/40"
                }`}
              >
                <span className={`font-mono text-[10px] ${focused ? "text-yellow-300" : "text-muted-foreground"}`}>
                  @@ region {i + 1} @@
                </span>
                <span className="font-mono text-[10px]">
                  <span className="text-blue-400">-{region.oursLines.length}</span>
                  <span className="text-muted-foreground/40 px-0.5">/</span>
                  <span className="text-rose-400">+{region.theirsLines.length}</span>
                </span>
                <span className="truncate text-[10px] text-muted-foreground/60">
                  {(region.oursLines[0] ?? region.theirsLines[0] ?? "").trim() || "(empty line)"}
                </span>
                <span className="ml-auto flex shrink-0 items-center gap-0.5">
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "ours"); }} className="rounded bg-blue-500/10 px-1.5 py-0.5 text-[9px] text-blue-300 hover:bg-blue-500/25 transition-colors" title="Accept ours for this region (O)">O</button>
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "theirs"); }} className="rounded bg-rose-500/10 px-1.5 py-0.5 text-[9px] text-rose-300 hover:bg-rose-500/25 transition-colors" title="Accept theirs for this region (T)">T</button>
                  <button onClick={(e) => { e.stopPropagation(); applyRegion(i, "both"); }} className="rounded bg-green-500/10 px-1.5 py-0.5 text-[9px] text-green-300 hover:bg-green-500/25 transition-colors" title="Keep both stacked (B)">B</button>
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* 3-pane — IntelliJ layout: Ours (left) · Merged (center, editable) ·
          Theirs (right). Two 18px ConnectorStrip columns sit between panes,
          rendering tinted SVG trapezoids that visually link each region's
          chunk in the source pane to its mapped position in the destination.
          All three panes share the same region focus and sync-scroll.
          `minmax(0,1fr)` (vs plain `1fr`) is critical: grid items default to
          `min-width: auto`, so without it CodeMirror's intrinsic line widths
          push the columns past 1/3 each and the rightmost panes overflow. */}
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_18px_minmax(0,1fr)_18px_minmax(0,1fr)] gap-0">
        <CodePane
          title="Ours"
          accent="text-blue-400"
          value={current.ours}
          readOnly
          filePath={path}
          wrap={inZen}
          highlightRanges={oursHighlights}
          scrollToLine={oursScrollTarget}
          onViewMount={setOursView}
        />
        <ConnectorStrip
          leftView={oursView}
          rightView={mergedView}
          specs={oursConnectorSpecs}
          tone="ours"
        />
        <CodePane
          title="Merged (editable)"
          accent="text-green-400"
          value={current.merged}
          onChange={updateMerged}
          hint="click to edit · O/T/B to accept"
          filePath={path}
          wrap={inZen}
          scrollToLine={mergedScrollTarget}
          highlightRanges={mergedHighlights}
          inlineActions={mergedInlineActions}
          onAccept={applyRegion}
          onViewMount={setMergedView}
        />
        <ConnectorStrip
          leftView={mergedView}
          rightView={theirsView}
          specs={theirsConnectorSpecs}
          tone="theirs"
        />
        <CodePane
          title="Theirs"
          accent="text-rose-400"
          value={current.theirs}
          readOnly
          filePath={path}
          wrap={inZen}
          highlightRanges={theirsHighlights}
          scrollToLine={theirsScrollTarget}
          onViewMount={setTheirsView}
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

      {/* File picker overlay — same shape as PrViewer's. Floats over the
          panel, j/k+Enter nav, Esc closes. Backdrop dims the merge surface
          so the picker reads as a focused modal. */}
      {pickerOpen && files.length > 0 && (
        <>
          <div
            className="absolute inset-0 z-30 backdrop-blur-sm bg-black/30"
            onClick={() => setPickerOpen(false)}
          />
          <div className="absolute inset-0 z-40 flex items-start justify-center px-6 pt-12 pointer-events-none">
            <div className="pointer-events-auto w-full max-w-md max-h-[60vh] overflow-y-auto rounded border border-border bg-card shadow-2xl">
              <div className="sticky top-0 flex items-center justify-between border-b border-border/50 bg-card px-3 py-1.5">
                <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  Conflicted files ({files.length})
                </span>
                <span className="text-[9px] text-muted-foreground/60">
                  j/k move · Enter pick · Esc close
                </span>
              </div>
              {files.map((fp, i) => {
                const isCursor = pickerCursor === i;
                const isSelected = path === fp;
                return (
                  <button
                    key={fp}
                    type="button"
                    onMouseEnter={() => setPickerCursor(i)}
                    onClick={() => {
                      if (onPickFile) onPickFile(fp);
                      setPickerOpen(false);
                    }}
                    ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                    className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors border-l-2 ${
                      isCursor
                        ? "border-l-orange-500 bg-orange-500/10"
                        : "border-l-transparent hover:bg-secondary/30"
                    }`}
                  >
                    <AlertTriangle size={10} className={`shrink-0 ${isSelected ? "text-yellow-400" : "text-yellow-400/60"}`} />
                    <span className={`flex-1 truncate font-mono text-[11px] ${isSelected ? "text-foreground" : "text-foreground/80"}`}>
                      {fp}
                    </span>
                    {isSelected && (
                      <span className="shrink-0 text-[9px] text-muted-foreground/60">current</span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

interface ConflictHighlight {
  fromLine: number;
  toLine: number;
  tone: "ours" | "theirs" | "marker";
  focused: boolean;
}

interface ConflictInlineAction {
  atLine: number;
  regionIdx: number;
  focused: boolean;
  oursCount: number;
  theirsCount: number;
}

interface ConnectorSpec {
  leftFrom: number;
  leftTo: number;
  rightFrom: number;
  rightTo: number;
  focused: boolean;
  regionIdx: number;
}

/// SVG overlay rendered in the gutter column between two CodePanes. For each
/// connector spec, draws a tinted trapezoid whose left edge spans the chunk
/// in `leftView` and whose right edge spans the chunk in `rightView` — the
/// shape visually links "this block in pane A goes there in pane B". We
/// re-render via a tick state bumped on rAF whenever either pane scrolls or
/// resizes, so the polygons follow the views in real time without React
/// thrashing on every scroll event. Pixel positions come from CodeMirror's
/// `lineBlockAt(pos).top/bottom` plus the scrollDOM bounding rects, so layout
/// changes (zen mode, splitter drags) are picked up automatically.
function ConnectorStrip({
  leftView,
  rightView,
  specs,
  tone,
}: {
  leftView: EditorView | null;
  rightView: EditorView | null;
  specs: ConnectorSpec[];
  tone: "ours" | "theirs";
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [, setTick] = useState(0);

  useEffect(() => {
    if (!leftView || !rightView) return;
    let scheduled = false;
    function schedule() {
      if (scheduled) return;
      scheduled = true;
      requestAnimationFrame(() => {
        scheduled = false;
        setTick((t) => t + 1);
      });
    }
    leftView.scrollDOM.addEventListener("scroll", schedule, { passive: true });
    rightView.scrollDOM.addEventListener("scroll", schedule, { passive: true });
    const ro = new ResizeObserver(schedule);
    ro.observe(leftView.scrollDOM);
    ro.observe(rightView.scrollDOM);
    if (containerRef.current) ro.observe(containerRef.current);
    schedule();
    return () => {
      leftView.scrollDOM.removeEventListener("scroll", schedule);
      rightView.scrollDOM.removeEventListener("scroll", schedule);
      ro.disconnect();
    };
  }, [leftView, rightView]);

  // Build polygons every render — cheap (max ~10s of regions) and the
  // dependencies change on every scroll tick anyway. Dispatching off-screen
  // polygons is fine; we filter them out so the SVG stays small.
  let polygons: { points: string; focused: boolean; regionIdx: number }[] = [];
  if (leftView && rightView && containerRef.current) {
    const stripRect = containerRef.current.getBoundingClientRect();
    const leftRect = leftView.scrollDOM.getBoundingClientRect();
    const rightRect = rightView.scrollDOM.getBoundingClientRect();
    const leftScrollTop = leftView.scrollDOM.scrollTop;
    const rightScrollTop = rightView.scrollDOM.scrollTop;
    const stripWidth = stripRect.width;
    const stripHeight = stripRect.height;

    function viewBlock(view: EditorView, line: number): { top: number; bottom: number } | null {
      const doc = view.state.doc;
      if (line < 0 || line >= doc.lines) return null;
      try {
        const block = view.lineBlockAt(doc.line(line + 1).from);
        return { top: block.top, bottom: block.bottom };
      } catch {
        return null;
      }
    }

    polygons = specs
      .map((s) => {
        const lFrom = viewBlock(leftView, s.leftFrom);
        const lTo = viewBlock(leftView, s.leftTo);
        const rFrom = viewBlock(rightView, s.rightFrom);
        const rTo = viewBlock(rightView, s.rightTo);
        if (!lFrom || !lTo || !rFrom || !rTo) return null;

        const lTopY = leftRect.top + (lFrom.top - leftScrollTop) - stripRect.top;
        const lBotY = leftRect.top + (lTo.bottom - leftScrollTop) - stripRect.top;
        const rTopY = rightRect.top + (rFrom.top - rightScrollTop) - stripRect.top;
        const rBotY = rightRect.top + (rTo.bottom - rightScrollTop) - stripRect.top;

        // Drop polygons that fall entirely above or below the visible strip.
        if (Math.max(lBotY, rBotY) < 0) return null;
        if (Math.min(lTopY, rTopY) > stripHeight) return null;

        const points = `0,${lTopY.toFixed(1)} ${stripWidth.toFixed(1)},${rTopY.toFixed(1)} ${stripWidth.toFixed(1)},${rBotY.toFixed(1)} 0,${lBotY.toFixed(1)}`;
        return { points, focused: s.focused, regionIdx: s.regionIdx };
      })
      .filter((p): p is NonNullable<typeof p> => p !== null);
  }

  const baseFill = tone === "ours" ? "59, 130, 246" : "244, 63, 94";
  const baseStroke = tone === "ours" ? "59, 130, 246" : "244, 63, 94";

  return (
    <div ref={containerRef} className="relative pointer-events-none overflow-hidden border-r border-l border-border/50 bg-background/30">
      <svg className="absolute inset-0" width="100%" height="100%">
        {polygons.map((p) => (
          <polygon
            key={p.regionIdx}
            points={p.points}
            fill={`rgba(${baseFill}, ${p.focused ? 0.32 : 0.18})`}
            stroke={`rgba(${baseStroke}, ${p.focused ? 0.55 : 0.35})`}
            strokeWidth={p.focused ? 1 : 0.5}
          />
        ))}
      </svg>
    </div>
  );
}

/// Code pane with CodeMirror (syntax highlighting + line numbers).
function CodePane({ title, accent, value, onChange, readOnly, hint, filePath, wrap = false, scrollToLine, highlightRanges, inlineActions, onAccept, onViewMount }: {
  title: string;
  accent: string;
  value: string;
  onChange?: (v: string) => void;
  readOnly?: boolean;
  hint?: string;
  filePath: string;
  wrap?: boolean;
  /// Line number (0-indexed) to scroll into view. Bumping this prop (e.g.,
  /// to a fresh `{value, line}` tuple via parent state) re-issues the scroll
  /// so navigating regions actually drags the editor with the user.
  scrollToLine?: { line: number; nonce: number } | null;
  /// IntelliJ-style line tints for conflict regions. Each entry colors a
  /// contiguous line range with the chosen tone. Focused regions render
  /// brighter so the user sees which one j/k/click selected.
  highlightRanges?: ConflictHighlight[];
  /// Block widgets to render above the given line. Used by the merged pane
  /// to anchor per-conflict accept buttons (O/T/B) directly to the chunk.
  inlineActions?: ConflictInlineAction[];
  /// Fired by the inline action buttons. Same contract as the row-list
  /// buttons in ConflictView so both surfaces drive the same updateMerged.
  onAccept?: (regionIdx: number, choice: "ours" | "theirs" | "both") => void;
  /// Notifies the parent when this pane's EditorView mounts/unmounts. Used
  /// by ConflictView to wire scroll sync and feed the ConnectorStrip overlays
  /// — both need access to the live view, and a callback (vs ref) keeps React
  /// in the loop so connector polygons re-render when views become available.
  onViewMount?: (view: EditorView | null) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const editableCompartment = useRef(new Compartment());
  const decorationsCompartment = useRef(new Compartment());
  // Buttons inside the widget DOM read this ref so clicks always fire the
  // freshest applyRegion — without it, a stale onAccept captured at mount
  // would hold an outdated `current.merged` and apply against the wrong
  // baseline once the user starts resolving regions.
  const onAcceptRef = useRef(onAccept);
  onAcceptRef.current = onAccept;

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
      decorationsCompartment.current.of(EditorView.decorations.of(Decoration.none)),
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
    onViewMount?.(view);

    return () => {
      view.destroy();
      viewRef.current = null;
      onViewMount?.(null);
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

  // Scroll the editor to a requested line whenever the parent bumps the
  // nonce — used by ConflictView to drag the merged pane to whichever
  // region the user just focused. Without this, focusing a far region
  // updated the header counter but the editor kept showing the previous
  // viewport.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !scrollToLine) return;
    const doc = view.state.doc;
    const lineIdx = scrollToLine.line + 1;
    if (lineIdx < 1 || lineIdx > doc.lines) return;
    const pos = doc.line(lineIdx).from;
    view.dispatch({
      effects: EditorView.scrollIntoView(pos, { y: "center" }),
    });
  }, [scrollToLine]);

  // Rebuild conflict decorations (line tints + inline action widgets) on any
  // change to highlightRanges/inlineActions/value. We sort ranges by `from`
  // and dedupe widget+line decorations into a single RangeSet — CodeMirror
  // requires decorations to be added in document order. The compartment
  // reconfigure swaps the entire set atomically so partial states never flash.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const doc = view.state.doc;
    const builder = new RangeSetBuilder<Decoration>();
    type Entry =
      | { kind: "line"; from: number; cls: string }
      | { kind: "widget"; from: number; widget: AcceptActionsWidget };
    const entries: Entry[] = [];

    for (const h of highlightRanges ?? []) {
      const from = Math.max(0, h.fromLine);
      const to = Math.min(doc.lines - 1, h.toLine);
      for (let l = from; l <= to; l++) {
        const lineFrom = doc.line(l + 1).from;
        entries.push({
          kind: "line",
          from: lineFrom,
          cls: `cluihud-conflict-line tone-${h.tone}${h.focused ? " is-focused" : ""}`,
        });
      }
    }

    for (const a of inlineActions ?? []) {
      if (a.atLine < 0 || a.atLine >= doc.lines) continue;
      const lineFrom = doc.line(a.atLine + 1).from;
      entries.push({
        kind: "widget",
        from: lineFrom,
        widget: new AcceptActionsWidget(
          a.regionIdx,
          a.focused,
          a.oursCount,
          a.theirsCount,
          (idx, choice) => onAcceptRef.current?.(idx, choice),
        ),
      });
    }

    // Block widgets must precede line decorations at the same position so
    // the bar renders above the line. Within the same kind, ties broken by
    // insertion order. RangeSetBuilder requires strictly non-decreasing from.
    entries.sort((a, b) => {
      if (a.from !== b.from) return a.from - b.from;
      if (a.kind !== b.kind) return a.kind === "widget" ? -1 : 1;
      return 0;
    });

    for (const e of entries) {
      if (e.kind === "widget") {
        builder.add(e.from, e.from, Decoration.widget({ widget: e.widget, side: -1, block: true }));
      } else {
        builder.add(e.from, e.from, Decoration.line({ attributes: { class: e.cls } }));
      }
    }

    const decorations: DecorationSet = builder.finish();
    view.dispatch({
      effects: decorationsCompartment.current.reconfigure(EditorView.decorations.of(decorations)),
    });
  }, [highlightRanges, inlineActions, value]);

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
