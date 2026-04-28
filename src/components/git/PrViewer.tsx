import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import {
  prAnnotationsMapAtom,
  prAnnotationsKey,
  transitionAfterCleanupAction,
  type PrAnnotation,
  type PrChecks,
} from "@/stores/git";
import { activeSessionIdAtom, workspacesAtom } from "@/stores/workspace";
import { gitInfoMapAtom } from "@/stores/git";
import { activeTabAtom, type Tab } from "@/stores/rightPanel";
import { toastsAtom } from "@/stores/toast";
import { openConflictsTabAction } from "@/stores/conflict";
import {
  ChevronUp,
  ChevronDown,
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
  tabId: string;
}

export function PrViewer({ data, tabId }: PrViewerProps) {
  const { workspaceId, prNumber, title, state, url, baseRefName, headRefName } = data;

  const [hunks, setHunks] = useState<Hunk[]>([]);
  const [lines, setLines] = useState<DiffLine[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeHunk, setActiveHunk] = useState(0);
  const [checks, setChecks] = useState<PrChecks | null>(null);
  const [annotating, setAnnotating] = useState(false);
  const [annotationDraft, setAnnotationDraft] = useState("");
  const [merging, setMerging] = useState(false);
  const [mergeAnywayConfirm, setMergeAnywayConfirm] = useState(false);

  const scrollRef = useRef<HTMLDivElement>(null);
  const hunkRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const annotationInputRef = useRef<HTMLTextAreaElement>(null);

  const annotationsMap = useAtomValue(prAnnotationsMapAtom);
  const setAnnotationsMap = useSetAtom(prAnnotationsMapAtom);
  const activeSessionId = useAtomValue(activeSessionIdAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const gitInfoMap = useAtomValue(gitInfoMapAtom);
  const activeTab = useAtomValue(activeTabAtom);
  const addToast = useSetAtom(toastsAtom);
  const openConflictsTab = useSetAtom(openConflictsTabAction);
  const transitionAfterCleanup = useSetAtom(transitionAfterCleanupAction);

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
  const isPrTabFocused = activeTab?.id === tabId;

  const fetchDiff = useCallback(() => {
    setLoading(true);
    setError(null);
    invoke<string>("get_pr_diff", { workspaceId, prNumber })
      .then((text) => {
        const parsed = parsePrDiff(text);
        setLines(parsed.lines);
        setHunks(parsed.hunks);
        setActiveHunk(0);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [workspaceId, prNumber]);

  const fetchChecks = useCallback(() => {
    invoke<PrChecks | null>("get_pr_checks", { workspaceId, prNumber })
      .then((res) => setChecks(res))
      .catch(() => setChecks(null));
  }, [workspaceId, prNumber]);

  useEffect(() => {
    fetchDiff();
    fetchChecks();
  }, [fetchDiff, fetchChecks]);

  const navigateHunk = useCallback((direction: -1 | 1) => {
    setActiveHunk((prev) => {
      const next = Math.max(0, Math.min(hunks.length - 1, prev + direction));
      if (next === prev) return prev;
      const el = hunkRefs.current.get(next);
      if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
      return next;
    });
  }, [hunks.length]);

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
            openConflictsTab({ sessionId: owningSessionId });
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
  }, [workspaceId, prNumber, baseRefName, owningSessionId, addToast, openConflictsTab, transitionAfterCleanup]);

  function handleMergeClick() {
    if (state !== "OPEN") return;
    if (annotations.length > 0 && !mergeAnywayConfirm) {
      setMergeAnywayConfirm(true);
      return;
    }
    performMerge();
  }

  // Keyboard handling at body level: j/k + arrow nav, `a` annotate, Ctrl+Enter merge.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    function handleKeyDown(e: KeyboardEvent) {
      if (annotating) return;
      // Only act when the PR tab is the focused panel tab.
      if (!isPrTabFocused) return;

      if (e.code === "KeyJ" || e.code === "ArrowDown") {
        if (hunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        navigateHunk(1);
        return;
      }
      if (e.code === "KeyK" || e.code === "ArrowUp") {
        if (hunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        navigateHunk(-1);
        return;
      }
      if (e.code === "KeyA" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (hunks.length === 0) return;
        e.preventDefault();
        e.stopPropagation();
        startAnnotating();
        return;
      }
      if (e.code === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        e.stopPropagation();
        handleMergeClick();
      }
    }
    el.addEventListener("keydown", handleKeyDown, true);
    return () => el.removeEventListener("keydown", handleKeyDown, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hunks.length, annotating, isPrTabFocused, activeHunk, annotations.length, owningSessionId]);

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
    scrollRef.current?.focus();
  }, [tabId]);

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
          onClick={fetchDiff}
          className="mt-2 rounded bg-secondary px-3 py-1 text-[10px] text-foreground hover:bg-secondary/70"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
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
          {hunks.length > 0 && (
            <span className="ml-auto shrink-0 flex items-center gap-1">
              <span>{activeHunk + 1}/{hunks.length}</span>
              <button
                onClick={() => navigateHunk(-1)}
                disabled={activeHunk <= 0}
                className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary disabled:opacity-30"
                aria-label="Previous chunk (K)"
              >
                <ChevronUp size={10} />
              </button>
              <button
                onClick={() => navigateHunk(1)}
                disabled={activeHunk >= hunks.length - 1}
                className="flex size-4 items-center justify-center rounded text-muted-foreground hover:bg-secondary disabled:opacity-30"
                aria-label="Next chunk (J)"
              >
                <ChevronDown size={10} />
              </button>
            </span>
          )}
        </div>
      </div>

      {/* Body */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto outline-none"
        tabIndex={-1}
        data-scrollable
      >
        {lines.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <span className="text-[11px] text-muted-foreground">No diff content</span>
          </div>
        ) : (
          <div className="font-mono text-[11px] leading-[18px]" role="region" aria-label={`PR #${prNumber} diff`}>
            {lines.map((line, i) => {
              if (line.type === "file") {
                return (
                  <div
                    key={i}
                    className="border-y border-border/50 bg-secondary/40 px-2 py-1 text-[11px] font-medium text-foreground/90 sticky top-0 z-10"
                  >
                    {line.content}
                  </div>
                );
              }

              if (line.type === "header") {
                const isActive = line.hunkIndex === activeHunk;
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
                      <span className={`text-[10px] truncate ${isActive ? "text-blue-400" : "text-muted-foreground"}`}>
                        {line.content}
                      </span>
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
          <button
            onClick={applyWithClaude}
            disabled={!isOwningSessionActive}
            className="flex h-6 items-center gap-1.5 rounded bg-amber-500/15 px-2 text-[10px] font-medium text-amber-300 hover:bg-amber-500/25 disabled:cursor-not-allowed disabled:opacity-50"
            title={
              !owningSessionId
                ? "No local session matches this PR's branch"
                : !isOwningSessionActive
                  ? "Switch to the session that owns this PR (its terminal will receive the prompt)"
                  : "Send annotations as a structured prompt to the session terminal"
            }
          >
            <Sparkles size={10} /> Apply with Claude
          </button>
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
