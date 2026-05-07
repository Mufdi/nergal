import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import {
  prAnnotationsMapAtom,
  prAnnotationsKey,
  transitionAfterCleanupAction,
  gitChipModeAtom,
  prFilesCacheAtom,
  selectedPrFileAtom,
  prDiffCacheMapAtom,
  PR_DIFF_TTL_MS,
  type PrAnnotation,
  type PrChecks,
} from "@/stores/git";
import { activeSessionIdAtom, workspacesAtom } from "@/stores/workspace";
import { gitInfoMapAtom } from "@/stores/git";
import type { Tab } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { selectedConflictFileMapAtom, conflictsZenOpenAtom } from "@/stores/conflict";
import { zenModeAtom, zenActiveZoneAtom, prZenAtom } from "@/stores/zenMode";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import {
  ChevronUp,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Loader2,
  GitMerge,
  MessageSquare,
  Sparkles,
  Trash2,
  X,
  AlertTriangle,
  Check,
  Circle,
  FolderOpen,
} from "lucide-react";
import { Kbd } from "@/components/ui/kbd";

type LineType = "add" | "remove" | "context" | "header" | "file";

interface DiffLine {
  type: LineType;
  content: string;
  oldNum: number | null;
  newNum: number | null;
  hunkIndex: number;
  filePath?: string;
}

interface Hunk {
  index: number;
  filePath: string;
  label: string;
  startLine: number;
}

/// Parse a multi-file unified diff (the format `gh pr diff` returns) into a
/// flat line list and a hunk index. File boundaries are emitted as `file`
/// rows so the renderer can show "── path/to/file.ts ──" separators.
function parsePrDiff(text: string): { lines: DiffLine[]; hunks: Hunk[] } {
  const lines: DiffLine[] = [];
  const hunks: Hunk[] = [];
  let oldNum = 0;
  let newNum = 0;
  let hunkIndex = -1;
  let currentFile = "";

  for (const raw of text.split("\n")) {
    if (raw.startsWith("diff --git ")) {
      const match = raw.match(/diff --git a\/(.+?) b\/(.+)/);
      if (match) {
        currentFile = match[2];
        lines.push({
          type: "file",
          content: currentFile,
          oldNum: null,
          newNum: null,
          hunkIndex: -1,
          filePath: currentFile,
        });
      }
      continue;
    }
    if (
      raw.startsWith("index ") ||
      raw.startsWith("--- ") ||
      raw.startsWith("+++ ") ||
      raw.startsWith("new file") ||
      raw.startsWith("deleted file") ||
      raw.startsWith("old mode") ||
      raw.startsWith("new mode") ||
      raw.startsWith("similarity index") ||
      raw.startsWith("rename from") ||
      raw.startsWith("rename to") ||
      raw.startsWith("Binary files")
    ) {
      continue;
    }

    if (raw.startsWith("@@")) {
      hunkIndex++;
      const match = raw.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/);
      if (match) {
        oldNum = parseInt(match[1], 10);
        newNum = parseInt(match[2], 10);
      }
      const label = match?.[3]?.trim() ?? "";
      hunks.push({
        index: hunkIndex,
        filePath: currentFile,
        label,
        startLine: lines.length,
      });
      lines.push({
        type: "header",
        content: label ? `@@ ${label}` : `@@ Line ${oldNum}`,
        oldNum: null,
        newNum: null,
        hunkIndex,
        filePath: currentFile,
      });
    } else if (raw.startsWith("+")) {
      lines.push({ type: "add", content: raw.slice(1), oldNum: null, newNum, hunkIndex, filePath: currentFile });
      newNum++;
    } else if (raw.startsWith("-")) {
      lines.push({ type: "remove", content: raw.slice(1), oldNum, newNum: null, hunkIndex, filePath: currentFile });
      oldNum++;
    } else {
      const content = raw.startsWith(" ") ? raw.slice(1) : raw;
      if (oldNum > 0 || newNum > 0) {
        lines.push({ type: "context", content, oldNum, newNum, hunkIndex, filePath: currentFile });
        oldNum++;
        newNum++;
      }
    }
  }

  return { lines, hunks };
}

const LINE_BG: Record<LineType, string> = {
  add: "bg-green-500/10",
  remove: "bg-red-500/10",
  context: "",
  header: "",
  file: "",
};

const LINE_TEXT: Record<LineType, string> = {
  add: "text-green-400",
  remove: "text-red-400",
  context: "text-foreground/70",
  header: "text-muted-foreground",
  file: "text-foreground/90",
};

interface PrTabData {
  workspaceId: string;
  prNumber: number;
  title: string;
  state: string;
  url: string;
  baseRefName: string;
  headRefName: string;
  updatedAt: string;
}

interface PrViewerProps {
  data: PrTabData;
  /// Gates the keyboard listener. Pass `false` while the viewer is mounted
  /// but not the focused surface (e.g. parent re-renders behind a modal).
  /// Defaults to `true` since the viewer is typically only mounted when it
  /// is the active surface (chip body or active tab).
  isActive?: boolean;
  /// Set `true` when the viewer is rendered inside Zen's viewer zone. The
  /// listener gate then keys off `zenActiveZoneAtom === "viewer"` instead
  /// of bailing on any open Zen overlay.
  inZen?: boolean;
  /// Open the file picker overlay on first mount. PrsChip flips this on when
  /// the user presses Enter on a PR row so they pick which file to read
  /// instead of being dropped onto the diff's first file.
  defaultPickerOpen?: boolean;
}

export function PrViewer({ data, isActive = true, inZen = false, defaultPickerOpen = false }: PrViewerProps) {
  const { workspaceId, prNumber, title, state, url, baseRefName, headRefName } = data;

  const [hunks, setHunks] = useState<Hunk[]>([]);
  const [lines, setLines] = useState<DiffLine[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const prDiffCacheMap = useAtomValue(prDiffCacheMapAtom);
  const setPrDiffCacheMap = useSetAtom(prDiffCacheMapAtom);
  const [activeHunk, setActiveHunk] = useState(0);
  const [collapsedHunks, setCollapsedHunks] = useState<Set<number>>(new Set());
  const [checks, setChecks] = useState<PrChecks | null>(null);
  const [annotating, setAnnotating] = useState(false);
  const [annotationDraft, setAnnotationDraft] = useState("");
  const [merging, setMerging] = useState(false);
  const [mergeAnywayConfirm, setMergeAnywayConfirm] = useState(false);
  /// Per-PR active file. Lifted to a global atom keyed by PR identity so the
  /// Zen sidebar can show the same file list and stay in sync with the
  /// viewer's selection. PrViewer mirrors the Diff panel's "one file at a
  /// time" pattern; the picker (Ctrl+Shift+K) swaps files.
  const prKey = prAnnotationsKey(workspaceId, prNumber);
  const [selectedPrMap, setSelectedPrMap] = useAtom(selectedPrFileAtom);
  const selectedFile = selectedPrMap[prKey] ?? null;
  const setSelectedFile = useCallback(
    (next: string | null) => {
      setSelectedPrMap((prev) => ({ ...prev, [prKey]: next }));
    },
    [setSelectedPrMap, prKey],
  );
  const setPrFilesCache = useSetAtom(prFilesCacheAtom);
  const [pickerOpen, setPickerOpen] = useState(defaultPickerOpen);
  const [pickerCursor, setPickerCursor] = useState(0);

  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const hunkRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const annotationInputRef = useRef<HTMLTextAreaElement>(null);

  const annotationsMap = useAtomValue(prAnnotationsMapAtom);
  const setAnnotationsMap = useSetAtom(prAnnotationsMapAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const gitInfoMap = useAtomValue(gitInfoMapAtom);
  const addToast = useSetAtom(toastsAtom);
  const setSelectedConflictMap = useSetAtom(selectedConflictFileMapAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);
  const transitionAfterCleanup = useSetAtom(transitionAfterCleanupAction);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const zenZone = useAtomValue(zenActiveZoneAtom);
  const anyZenOpen = zenState.open || conflictsZen || prZen !== null;
  const listenerActive = inZen
    ? anyZenOpen && zenZone === "viewer"
    : !anyZenOpen;

  const annotationsKey = useMemo(
    () => prAnnotationsKey(workspaceId, prNumber),
    [workspaceId, prNumber],
  );
  const annotations = useMemo<PrAnnotation[]>(
    () => annotationsMap[annotationsKey] ?? [],
    [annotationsMap, annotationsKey],
  );

  /// Find the session in the workspace whose worktree branch matches the PR's
  /// head ref. Used to gate Apply-with-Claude (so we don't write a prompt
  /// into a session whose worktree has nothing to do with the PR).
  const owningSessionId = useMemo<string | null>(() => {
    const ws = workspaces.find((w) => w.id === workspaceId);
    if (!ws) return null;
    for (const s of ws.sessions) {
      const info = gitInfoMap[s.id];
      if (info && info.branch === headRefName) return s.id;
    }
    return null;
  }, [workspaces, workspaceId, headRefName, gitInfoMap]);

  const isOwningSessionActive = owningSessionId !== null && owningSessionId === activeSessionId;

  /// Fetch + parse the PR diff. Cache the raw text by `${workspaceId}:${prNumber}`
  /// so flipping back to a recently-viewed PR is instant. The parse step
  /// runs each time (it's local and cheap relative to the network call,
  /// and avoids holding both raw + parsed forms in the cache).
  const fetchDiff = useCallback((opts: { background?: boolean } = {}) => {
    if (!opts.background) setLoading(true);
    setError(null);
    invoke<string>("get_pr_diff", { workspaceId, prNumber })
      .then((text) => {
        const cacheKey = `${workspaceId}:${prNumber}`;
        setPrDiffCacheMap((prev) => ({ ...prev, [cacheKey]: { text, fetchedAt: Date.now() } }));
        const parsed = parsePrDiff(text);
        setLines(parsed.lines);
        setHunks(parsed.hunks);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [workspaceId, prNumber, setPrDiffCacheMap]);

  /// Unique file paths the PR touches, in the order the diff parser emitted
  /// them. Drives the file picker and the per-file filter that hides
  /// non-selected hunks/lines from the render pass.
  const prFiles = useMemo<string[]>(() => {
    const seen = new Set<string>();
    const list: string[] = [];
    for (const h of hunks) {
      if (!seen.has(h.filePath)) {
        seen.add(h.filePath);
        list.push(h.filePath);
      }
    }
    return list;
  }, [hunks]);

  /// +/- counts per file, derived once from the parsed lines so the picker
  /// can show "M src/foo.ts +12 -3" badges without re-walking on every key.
  const fileStats = useMemo<Record<string, { adds: number; removes: number }>>(() => {
    const stats: Record<string, { adds: number; removes: number }> = {};
    for (const line of lines) {
      const fp = line.filePath;
      if (!fp || (line.type !== "add" && line.type !== "remove")) continue;
      if (!stats[fp]) stats[fp] = { adds: 0, removes: 0 };
      if (line.type === "add") stats[fp].adds += 1;
      else stats[fp].removes += 1;
    }
    return stats;
  }, [lines]);

  const visibleHunks = useMemo<Hunk[]>(
    () => (selectedFile ? hunks.filter((h) => h.filePath === selectedFile) : []),
    [hunks, selectedFile],
  );

  // Snap activeHunk into visibleHunks whenever the file changes so j/k starts
  // from the top of the new file's diff.
  useEffect(() => {
    if (visibleHunks.length === 0) return;
    if (!visibleHunks.find((h) => h.index === activeHunk)) {
      setActiveHunk(visibleHunks[0].index);
    }
  }, [visibleHunks, activeHunk]);

  // Default-select the first file when the diff loads and nothing is picked.
  // Skipped if the atom already remembers a previous selection — closing and
  // reopening the same PR keeps the user where they were.
  useEffect(() => {
    if (selectedFile) return;
    if (prFiles.length === 0) return;
    setSelectedFile(prFiles[0]);
  }, [selectedFile, prFiles, setSelectedFile]);

  // Publish the parsed file list + per-file +/- counts to the global cache so
  // the Zen sidebar (a separate component tree) can render the same list
  // without re-fetching the diff.
  useEffect(() => {
    if (prFiles.length === 0) return;
    const entries = prFiles.map((p) => ({
      path: p,
      adds: fileStats[p]?.adds ?? 0,
      removes: fileStats[p]?.removes ?? 0,
    }));
    setPrFilesCache((prev) => ({ ...prev, [prKey]: entries }));
  }, [prFiles, fileStats, prKey, setPrFilesCache]);

  const fetchChecks = useCallback(() => {
    invoke<PrChecks | null>("get_pr_checks", { workspaceId, prNumber })
      .then((res) => setChecks(res))
      .catch(() => setChecks(null));
  }, [workspaceId, prNumber]);

  /// Hydrate from the diff cache on mount: re-opening the same PR within
  /// PR_DIFF_TTL_MS skips the network round-trip and the spinner. A stale
  /// cache still seeds the parsed view immediately while a background
  /// refresh fetches the latest diff in place.
  useEffect(() => {
    const cacheKey = `${workspaceId}:${prNumber}`;
    const cached = prDiffCacheMap[cacheKey];
    if (cached) {
      const parsed = parsePrDiff(cached.text);
      setLines(parsed.lines);
      setHunks(parsed.hunks);
      const fresh = Date.now() - cached.fetchedAt < PR_DIFF_TTL_MS;
      if (!fresh) fetchDiff({ background: true });
    } else {
      fetchDiff();
    }
    fetchChecks();
    // prDiffCacheMap intentionally omitted: read for the staleness gate,
    // must not retrigger on cache mutation (would cause refetch loop).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, prNumber, fetchDiff, fetchChecks]);

  const navigateHunk = useCallback((direction: -1 | 1) => {
    setActiveHunk((prev) => {
      const visible = visibleHunks;
      if (visible.length === 0) return prev;
      const idx = visible.findIndex((h) => h.index === prev);
      const nextIdx = idx === -1
        ? 0
        : Math.max(0, Math.min(visible.length - 1, idx + direction));
      const next = visible[nextIdx]?.index ?? prev;
      if (next === prev) return prev;
      const el = hunkRefs.current.get(next);
      if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
      return next;
    });
  }, [visibleHunks]);

  const toggleCollapse = useCallback((index: number) => {
    setCollapsedHunks((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  function startAnnotating() {
    const existing = annotations.find((a) => a.hunkIndex === activeHunk);
    setAnnotationDraft(existing?.text ?? "");
    setAnnotating(true);
    requestAnimationFrame(() => annotationInputRef.current?.focus());
  }

  function cancelAnnotating() {
    setAnnotating(false);
    setAnnotationDraft("");
    requestAnimationFrame(() => scrollRef.current?.focus());
  }

  function saveAnnotation() {
    const trimmed = annotationDraft.trim();
    setAnnotationsMap((prev) => {
      const list = prev[annotationsKey] ?? [];
      const filtered = list.filter((a) => a.hunkIndex !== activeHunk);
      if (trimmed.length === 0) {
        return { ...prev, [annotationsKey]: filtered };
      }
      const newAnn: PrAnnotation = {
        id: `${activeHunk}-${Date.now()}`,
        hunkIndex: activeHunk,
        text: trimmed,
      };
      return { ...prev, [annotationsKey]: [...filtered, newAnn] };
    });
    setAnnotating(false);
    setAnnotationDraft("");
    requestAnimationFrame(() => scrollRef.current?.focus());
  }

  function deleteAnnotation(id: string) {
    setAnnotationsMap((prev) => {
      const list = prev[annotationsKey] ?? [];
      return { ...prev, [annotationsKey]: list.filter((a) => a.id !== id) };
    });
  }

  function clearAllAnnotations() {
    setAnnotationsMap((prev) => {
      const copy = { ...prev };
      delete copy[annotationsKey];
      return copy;
    });
  }

  function applyWithClaude() {
    if (!isOwningSessionActive || annotations.length === 0) return;
    const sid = owningSessionId;
    if (!sid) return;

    const sorted = [...annotations].sort((a, b) => a.hunkIndex - b.hunkIndex);
    const blocks = sorted.map((a) => {
      const hunk = hunks.find((h) => h.index === a.hunkIndex);
      const anchor = hunk
        ? `${hunk.filePath} @@ ${hunk.label || `hunk ${hunk.index + 1}`}`
        : `hunk ${a.hunkIndex + 1}`;
      return `### ${anchor}\n${a.text}`;
    });

    const prompt = [
      `Apply the following review feedback to PR #${prNumber} (${title}).`,
      `Each block names the file + chunk anchor, then the requested change.`,
      `Edit the worktree, run any local checks, then commit and push when done.`,
      ``,
      ...blocks,
      ``,
      `Once you've pushed, I'll refresh the PR Viewer and resolve the annotations.`,
    ].join("\n");

    invoke("terminal_paste", { sessionId: sid, text: prompt })
      .then(() => {
        addToast({
          message: "Apply with Claude",
          description: `Sent ${annotations.length} annotation${annotations.length === 1 ? "" : "s"} to the session terminal.`,
          type: "success",
        });
      })
      .catch((err: unknown) => {
        addToast({
          message: "Apply failed",
          description: String(err),
          type: "error",
        });
      });
  }

  const performMerge = useCallback(() => {
    setMerging(true);
    setMergeAnywayConfirm(false);
    invoke<void>("gh_pr_merge", { workspaceId, prNumber })
      .then(() => {
        addToast({
          message: "PR merged",
          description: `PR #${prNumber} merged into ${baseRefName}.`,
          type: "success",
        });
        if (owningSessionId) {
          invoke<{ deleted: boolean; warnings: string[]; archived_plans_path: string | null }>(
            "cleanup_merged_session",
            { sessionId: owningSessionId },
          )
            .then((res) => transitionAfterCleanup({
              deletedSessionId: owningSessionId,
              workspaceId,
              warnings: res.warnings,
              archivedPlansPath: res.archived_plans_path,
            }))
            .catch((err: unknown) => {
              addToast({
                message: "Cleanup failed",
                description: String(err),
                type: "error",
              });
            });
        } else {
          addToast({
            message: "Cleanup skipped",
            description: "No local session matched this PR's branch — nothing to delete.",
            type: "info",
          });
        }
      })
      .catch((err: unknown) => {
        const msg = String(err);
        if (msg.includes("mergeable=false")) {
          addToast({
            message: "Merge blocked",
            description: "PR has conflicts. Opening the conflicts tab.",
            type: "info",
          });
          if (owningSessionId) {
            setChipModeMap((prev) => ({ ...prev, [owningSessionId]: "conflicts" }));
            // Clear pre-selection so ConflictsPanel falls back to the first
            // conflicted file from the new state.
            setSelectedConflictMap((prev) => {
              const copy = { ...prev };
              delete copy[owningSessionId];
              return copy;
            });
          }
        } else {
          addToast({
            message: "Merge failed",
            description: msg,
            type: "error",
          });
        }
      })
      .finally(() => setMerging(false));
  }, [workspaceId, prNumber, baseRefName, owningSessionId, addToast, setChipModeMap, setSelectedConflictMap, transitionAfterCleanup]);

  function handleMergeClick() {
    if (state !== "OPEN") return;
    if (annotations.length > 0 && !mergeAnywayConfirm) {
      setMergeAnywayConfirm(true);
      return;
    }
    performMerge();
  }

  // Keyboard handling at window level, gated by `isActive` and Zen state.
  // State-based gating (no DOM focus dependency): exactly one PrViewer
  // listener is live at any moment, regardless of which element holds focus.
  useEffect(() => {
    if (!isActive) return;
    if (!listenerActive) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (annotating) return;
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;

      // Picker open: j/k drives the picker cursor, Enter commits, Esc closes.
      if (pickerOpen) {
        if (e.code === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          setPickerOpen(false);
          return;
        }
        if (prFiles.length === 0) return;
        if (e.code === "KeyJ" || e.code === "ArrowDown") {
          e.preventDefault();
          e.stopPropagation();
          setPickerCursor((i) => (i + 1) % prFiles.length);
          return;
        }
        if (e.code === "KeyK" || e.code === "ArrowUp") {
          e.preventDefault();
          e.stopPropagation();
          setPickerCursor((i) => (i - 1 + prFiles.length) % prFiles.length);
          return;
        }
        if (e.code === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          const pick = prFiles[pickerCursor];
          if (pick) {
            setSelectedFile(pick);
            setPickerOpen(false);
          }
          return;
        }
        return;
      }

      if (e.code === "KeyJ" || e.code === "ArrowDown") {
        if (visibleHunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        navigateHunk(1);
        return;
      }
      if (e.code === "KeyK" || e.code === "ArrowUp") {
        if (visibleHunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        navigateHunk(-1);
        return;
      }
      if (e.code === "Space" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
        if (visibleHunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        toggleCollapse(activeHunk);
        return;
      }
      if (e.code === "KeyA" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (visibleHunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        startAnnotating();
        return;
      }
      if (e.code === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        handleMergeClick();
        return;
      }
      // Ctrl+←/→ — file prev/next within the PR's file list.
      if (e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && (e.code === "ArrowLeft" || e.code === "ArrowRight")) {
        if (prFiles.length < 2 || !selectedFile) return;
        e.preventDefault();
        e.stopPropagation();
        const idx = prFiles.indexOf(selectedFile);
        if (idx === -1) return;
        const nextIdx = e.code === "ArrowRight"
          ? (idx + 1) % prFiles.length
          : (idx - 1 + prFiles.length) % prFiles.length;
        const next = prFiles[nextIdx];
        if (next) setSelectedFile(next);
        return;
      }
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visibleHunks, annotating, isActive, activeHunk, annotations.length, owningSessionId, toggleCollapse, listenerActive, pickerOpen, prFiles, pickerCursor, selectedFile, setSelectedFile]);

  // External Ctrl+Shift+K → toggle the picker. When opened, snap the picker
  // cursor to the currently-selected file so j/k starts from a useful spot.
  useEffect(() => {
    if (!isActive || !listenerActive) return;
    function onToggle() {
      setPickerOpen((open) => {
        if (!open) {
          const idx = selectedFile ? prFiles.indexOf(selectedFile) : -1;
          setPickerCursor(idx >= 0 ? idx : 0);
        }
        return !open;
      });
    }
    document.addEventListener("cluihud:toggle-file-picker", onToggle);
    return () => document.removeEventListener("cluihud:toggle-file-picker", onToggle);
  }, [isActive, listenerActive, selectedFile, prFiles]);

  // Ctrl+Shift+R routes here when the user is on the PRs chip (see the
  // revise-or-resolve handler in shortcuts.ts). applyWithClaude is the same
  // path the footer button uses; it no-ops when there are no annotations or
  // the owning session isn't active, so the shortcut is safe to fire blind.
  useEffect(() => {
    if (!isActive || !listenerActive) return;
    function onApply() { applyWithClaude(); }
    document.addEventListener("cluihud:apply-pr-annotations", onApply);
    return () => document.removeEventListener("cluihud:apply-pr-annotations", onApply);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActive, listenerActive, annotations.length, owningSessionId, isOwningSessionActive]);

  // Steal focus when the panel zone wrapper gets focused (Alt+Left/Right).
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const panel = el.closest("[data-focus-zone='panel']") as HTMLElement | null;
    if (!panel) return;
    function handleFocus(e: FocusEvent) {
      if (e.target === panel) scrollRef.current?.focus();
    }
    panel.addEventListener("focus", handleFocus, true);
    return () => panel.removeEventListener("focus", handleFocus, true);
  }, []);

  useEffect(() => {
    if (isActive) {
      // Focus the container so the keyboard listener's containment check
      // passes immediately on mount; the scroll body still owns wheel/scroll
      // events because focus inside the container is enough.
      containerRef.current?.focus();
    }
  }, [isActive, prNumber]);

  if (loading && lines.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <Loader2 size={16} className="animate-spin text-muted-foreground" />
        <span className="text-[11px] text-muted-foreground">Loading PR #{prNumber}…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-4">
        <AlertTriangle size={20} className="text-red-400" />
        <span className="text-[11px] text-red-400 text-center">{error}</span>
        <button
          onClick={() => fetchDiff()}
          className="mt-2 rounded bg-secondary px-3 py-1 text-[10px] text-foreground hover:bg-secondary/70"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div ref={containerRef} tabIndex={-1} className="flex h-full flex-col outline-none">
      {/* Header */}
      <div className="flex shrink-0 flex-col gap-1 border-b border-border/50 px-3 py-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${
            state === "OPEN"
              ? "bg-green-500/15 text-green-400"
              : state === "MERGED"
                ? "bg-purple-500/15 text-purple-400"
                : "bg-muted text-muted-foreground"
          }`}>
            PR #{prNumber}
          </span>
          <span className="truncate text-[11px] font-medium text-foreground/90" title={title}>
            {title}
          </span>
          <a
            href={url}
            target="_blank"
            rel="noopener noreferrer"
            className="ml-auto flex shrink-0 items-center text-muted-foreground hover:text-foreground"
            title="Open on GitHub"
          >
            <ExternalLink size={11} />
          </a>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-muted-foreground font-mono">
          <span className="truncate">{headRefName}</span>
          <ChevronRight size={10} className="shrink-0" />
          <span className="truncate">{baseRefName}</span>
          {checks && checks.total > 0 && (
            <CiPill checks={checks} />
          )}
        </div>
        {/* File-picker bar — mirrors the Diff panel: one file at a time, swap
            via Ctrl+Shift+K. Without this, multi-file PRs stack `diff --git`
            sticky headers on collapse and the user can't tell which chunk
            belongs to which file. */}
        {selectedFile && (
          <div className="flex items-center gap-1.5 text-[10px]">
            {prFiles.length > 1 && (
              <>
                <button
                  type="button"
                  onClick={() => {
                    const idx = prFiles.indexOf(selectedFile);
                    if (idx === -1) return;
                    const prev = prFiles[(idx - 1 + prFiles.length) % prFiles.length];
                    if (prev) setSelectedFile(prev);
                  }}
                  className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  aria-label="Previous file"
                  title="Previous file (Ctrl+←)"
                >
                  <ChevronLeft size={12} />
                </button>
                <button
                  type="button"
                  onClick={() => {
                    const idx = prFiles.indexOf(selectedFile);
                    if (idx === -1) return;
                    const next = prFiles[(idx + 1) % prFiles.length];
                    if (next) setSelectedFile(next);
                  }}
                  className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  aria-label="Next file"
                  title="Next file (Ctrl+→)"
                >
                  <ChevronRight size={12} />
                </button>
              </>
            )}
            <button
              type="button"
              onClick={() => {
                const idx = selectedFile ? prFiles.indexOf(selectedFile) : -1;
                setPickerCursor(idx >= 0 ? idx : 0);
                setPickerOpen((v) => !v);
              }}
              className="flex shrink-0 items-center gap-1 rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors px-1 py-0.5"
              title="Pick file (Ctrl+Shift+K)"
            >
              <FolderOpen size={11} />
              <span className="font-mono text-foreground/85 truncate max-w-[18rem]">{selectedFile}</span>
              <span className="text-muted-foreground/60 tabular-nums">
                {prFiles.length > 0 && `· ${prFiles.indexOf(selectedFile) + 1}/${prFiles.length}`}
              </span>
            </button>
            <span className="text-muted-foreground/40">·</span>
            <span className="font-mono">
              <span className="text-green-400">+{fileStats[selectedFile]?.adds ?? 0}</span>
              <span className="text-muted-foreground/40 px-0.5">/</span>
              <span className="text-red-400">-{fileStats[selectedFile]?.removes ?? 0}</span>
            </span>
            {visibleHunks.length > 0 && (
              <span className="ml-auto shrink-0 flex items-center gap-1">
                <span>{(visibleHunks.findIndex((h) => h.index === activeHunk) + 1) || 1}/{visibleHunks.length}</span>
                <button
                  onClick={() => navigateHunk(-1)}
                  className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary"
                  aria-label="Previous chunk (K)"
                >
                  <ChevronUp size={10} />
                </button>
                <button
                  onClick={() => navigateHunk(1)}
                  className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary"
                  aria-label="Next chunk (J)"
                >
                  <ChevronDown size={10} />
                </button>
              </span>
            )}
          </div>
        )}
      </div>

      {/* Body */}
      <div
        ref={scrollRef}
        className="relative flex-1 overflow-y-auto outline-none"
        tabIndex={-1}
        data-scrollable
      >
        {lines.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <span className="text-[11px] text-muted-foreground">No diff content</span>
          </div>
        ) : (
          <div className="font-mono text-[11px] leading-[18px]" role="region" aria-label={`PR #${prNumber} diff for ${selectedFile ?? ""}`}>
            {lines.map((line, i) => {
              // File boundary rows are no longer rendered — the file name is
              // shown in the header bar now. Skip non-selected files entirely
              // so the renderer only walks the active file's lines.
              if (line.type === "file") return null;
              if (selectedFile && line.filePath !== selectedFile) return null;

              if (line.type !== "header" && line.hunkIndex >= 0 && collapsedHunks.has(line.hunkIndex)) {
                return null;
              }

              if (line.type === "header") {
                const isActive = line.hunkIndex === activeHunk;
                const isCollapsed = collapsedHunks.has(line.hunkIndex);
                const annotation = annotations.find((a) => a.hunkIndex === line.hunkIndex);
                return (
                  <div key={i}>
                    <div
                      ref={(el) => { if (el) hunkRefs.current.set(line.hunkIndex, el); }}
                      onClick={() => setActiveHunk(line.hunkIndex)}
                      style={{ scrollMarginTop: "8px" }}
                      className={`flex cursor-pointer items-center gap-1 border-t px-2 py-0.5 transition-colors ${
                        isActive
                          ? "border-t-blue-500/40 border-l-2 border-l-blue-500 bg-blue-500/15"
                          : "border-t-border/30 bg-secondary/30 hover:bg-secondary/50"
                      }`}
                    >
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          toggleCollapse(line.hunkIndex);
                          (e.currentTarget as HTMLElement).blur();
                        }}
                        className="flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/70 hover:bg-secondary"
                        title={isCollapsed ? "Expand (Space)" : "Collapse (Space)"}
                      >
                        <ChevronRight size={10} className={`transition-transform ${isCollapsed ? "" : "rotate-90"}`} />
                      </button>
                      <span className={`text-[10px] truncate ${isActive ? "text-blue-400" : "text-muted-foreground"}`}>
                        {line.content}
                      </span>
                      {isCollapsed && (
                        <span className="text-[10px] text-muted-foreground/50">
                          collapsed
                        </span>
                      )}
                      {annotation && (
                        <MessageSquare size={10} className="ml-auto shrink-0 text-amber-400" />
                      )}
                    </div>
                    {annotation && !(annotating && line.hunkIndex === activeHunk) && (
                      <div className="flex items-start gap-2 border-l-2 border-amber-500/60 bg-amber-500/5 px-3 py-1.5">
                        <MessageSquare size={11} className="mt-0.5 shrink-0 text-amber-400" />
                        <span className="flex-1 whitespace-pre-wrap text-[11px] text-foreground/90 font-sans">
                          {annotation.text}
                        </span>
                        <button
                          onClick={() => deleteAnnotation(annotation.id)}
                          className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-red-400"
                          aria-label="Delete annotation"
                        >
                          <Trash2 size={10} />
                        </button>
                      </div>
                    )}
                    {annotating && line.hunkIndex === activeHunk && (
                      <div className="flex flex-col gap-1 border-l-2 border-amber-500/60 bg-amber-500/5 px-3 py-2">
                        <textarea
                          ref={annotationInputRef}
                          value={annotationDraft}
                          onChange={(e) => setAnnotationDraft(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Escape") {
                              e.preventDefault();
                              cancelAnnotating();
                            } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                              e.preventDefault();
                              saveAnnotation();
                            }
                          }}
                          placeholder="What should change in this chunk? (e.g. use Set instead of Array)"
                          rows={3}
                          className="w-full resize-none rounded bg-card border border-border/50 px-2 py-1 text-[11px] text-foreground/90 outline-none focus:border-amber-500/50 font-sans"
                        />
                        <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                          <button
                            onClick={saveAnnotation}
                            className="flex h-6 items-center gap-1 rounded bg-amber-500/20 px-2 text-amber-300 hover:bg-amber-500/30"
                          >
                            Save <Kbd keys="ctrl+enter" />
                          </button>
                          <button
                            onClick={cancelAnnotating}
                            className="flex h-6 items-center gap-1 rounded text-muted-foreground hover:bg-secondary"
                          >
                            <X size={10} /> Esc
                          </button>
                          <span className="ml-auto">Empty + save = delete</span>
                        </div>
                      </div>
                    )}
                  </div>
                );
              }

              const numColor = line.type === "add"
                ? "text-green-400/50"
                : line.type === "remove"
                  ? "text-red-400/50"
                  : "text-muted-foreground/30";

              return (
                <div key={i} className={`flex ${LINE_BG[line.type]}`}>
                  <span className={`inline-block w-10 shrink-0 select-none pr-1 text-right ${numColor}`}>
                    {line.oldNum ?? ""}
                  </span>
                  <span className={`inline-block w-10 shrink-0 select-none pr-1 text-right ${numColor}`}>
                    {line.newNum ?? ""}
                  </span>
                  <span className={`inline-block w-4 shrink-0 select-none text-center ${LINE_TEXT[line.type]}`}>
                    {line.type === "add" ? "+" : line.type === "remove" ? "-" : " "}
                  </span>
                  <span className={`flex-1 whitespace-pre ${LINE_TEXT[line.type]}`}>
                    {line.content}
                  </span>
                </div>
              );
            })}
          </div>
        )}

        {/* File picker overlay — backdrop dims the diff, picker floats over
            it with j/k+Enter nav. Mirrors the Diff panel's FilePickerOverlay
            shape so muscle memory carries over from one panel to the other. */}
        {pickerOpen && prFiles.length > 0 && (
          <>
            <div
              className="absolute inset-0 z-30 bg-scrim cluihud-blur-sm"
              onClick={() => setPickerOpen(false)}
            />
            <div className="absolute inset-0 z-40 flex items-start justify-center px-6 pt-12 pointer-events-none">
              <div className="pointer-events-auto w-full max-w-md max-h-[60vh] overflow-y-auto rounded border border-border bg-card shadow-lg">
                <div className="sticky top-0 flex items-center justify-between border-b border-border/50 bg-card px-3 py-1.5">
                  <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                    PR files ({prFiles.length})
                  </span>
                  <span className="text-[9px] text-muted-foreground/60">
                    j/k move · Enter pick · Esc close
                  </span>
                </div>
                {prFiles.map((fp, i) => {
                  const isCursor = pickerCursor === i;
                  const isSelected = selectedFile === fp;
                  const stats = fileStats[fp];
                  return (
                    <button
                      key={fp}
                      type="button"
                      onMouseEnter={() => setPickerCursor(i)}
                      onClick={() => { setSelectedFile(fp); setPickerOpen(false); }}
                      ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                      className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors border-l-2 ${
                        isCursor
                          ? "border-l-orange-500 bg-orange-500/10"
                          : "border-l-transparent hover:bg-secondary/30"
                      }`}
                    >
                      <span className={`flex-1 truncate font-mono text-[11px] ${isSelected ? "text-foreground" : "text-foreground/80"}`}>
                        {fp}
                      </span>
                      {stats && (
                        <span className="shrink-0 font-mono text-[10px]">
                          <span className="text-green-400">+{stats.adds}</span>
                          <span className="text-muted-foreground/40 px-0.5">/</span>
                          <span className="text-red-400">-{stats.removes}</span>
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            </div>
          </>
        )}
      </div>

      {/* Footer */}
      <div className="flex shrink-0 items-center gap-2 border-t border-border/50 px-3 py-1.5">
        {annotations.length > 0 && (
          <span className="flex items-center gap-1 text-[10px] text-amber-400">
            <MessageSquare size={10} />
            {annotations.length} annotation{annotations.length === 1 ? "" : "s"}
          </span>
        )}
        {annotations.length > 0 && (
          <button
            onClick={clearAllAnnotations}
            className="flex h-6 items-center gap-1 rounded text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
            title="Remove all annotations"
          >
            <Trash2 size={10} /> Clear
          </button>
        )}

        {annotations.length > 0 && (
          <Tooltip>
            <TooltipTrigger>
              <span className={!isOwningSessionActive ? "inline-block" : "inline-block"}>
                <button
                  onClick={applyWithClaude}
                  disabled={!isOwningSessionActive}
                  className="flex h-6 items-center gap-1.5 rounded bg-amber-500/15 px-2 text-[10px] font-medium text-amber-300 hover:bg-amber-500/25 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <Sparkles size={10} /> Apply with Claude ({annotations.length})
                  <Kbd keys="ctrl+shift+r" />
                </button>
              </span>
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-xs text-[10px]">
              {!owningSessionId
                ? `No local session matches this PR's branch (${headRefName}). Open a session whose worktree is on this branch to apply.`
                : !isOwningSessionActive
                  ? `Switch to the session on branch "${headRefName}" — its terminal will receive the prompt.`
                  : `Send ${annotations.length} annotation${annotations.length === 1 ? "" : "s"} as a structured prompt to the session terminal.`}
            </TooltipContent>
          </Tooltip>
        )}

        <div className="ml-auto flex items-center gap-2">
          {state !== "OPEN" && (
            <span className="text-[10px] text-muted-foreground">
              PR is {state.toLowerCase()}
            </span>
          )}
          {mergeAnywayConfirm ? (
            <>
              <span className="text-[10px] text-amber-400">
                {annotations.length} unresolved — merge anyway?
              </span>
              <button
                onClick={() => setMergeAnywayConfirm(false)}
                className="flex h-6 items-center rounded px-2 text-[10px] text-muted-foreground hover:bg-secondary"
              >
                Cancel
              </button>
              <button
                onClick={performMerge}
                disabled={merging}
                className="flex h-6 items-center gap-1 rounded bg-amber-500/20 px-2 text-[10px] font-medium text-amber-300 hover:bg-amber-500/30 disabled:opacity-50"
              >
                {merging ? <Loader2 size={10} className="animate-spin" /> : <GitMerge size={10} />}
                Merge anyway
              </button>
            </>
          ) : (
            <button
              onClick={handleMergeClick}
              disabled={merging || state !== "OPEN"}
              className="flex h-6 items-center gap-1.5 rounded bg-green-500/20 px-2 text-[10px] font-medium text-green-300 hover:bg-green-500/30 disabled:cursor-not-allowed disabled:opacity-50"
              title={state === "OPEN" ? `Merge PR #${prNumber} into ${baseRefName}` : "PR is not open"}
            >
              {merging ? <Loader2 size={10} className="animate-spin" /> : <GitMerge size={10} />}
              Merge into {baseRefName}
              <Kbd keys="ctrl+enter" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function CiPill({ checks }: { checks: PrChecks }) {
  if (checks.failing > 0) {
    return (
      <span className="flex shrink-0 items-center gap-0.5 rounded bg-red-500/15 px-1 text-[10px] text-red-400">
        <X size={9} /> {checks.failing}/{checks.total}
      </span>
    );
  }
  if (checks.pending > 0) {
    return (
      <span className="flex shrink-0 items-center gap-0.5 rounded bg-yellow-500/15 px-1 text-[10px] text-yellow-400">
        <Circle size={9} className="animate-pulse" /> {checks.pending}/{checks.total}
      </span>
    );
  }
  return (
    <span className="flex shrink-0 items-center gap-0.5 rounded bg-green-500/15 px-1 text-[10px] text-green-400">
      <Check size={9} /> {checks.passing}/{checks.total}
    </span>
  );
}

export type { PrTabData };
export type { Tab };
