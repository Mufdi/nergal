import { useState, useEffect, useCallback, useRef } from "react";
import { invoke, listen } from "@/lib/tauri";
import { refreshGitInfoAtom, refreshConflictedFilesAtom, activeConflictedFilesAtom, prChecksMapAtom, sessionsAutoMergedAtom, type PrChecks } from "@/stores/git";
import { openZenModeAtom } from "@/stores/zenMode";
import { triggerShipAtom } from "@/stores/ship";
import { triggerMergeAtom } from "@/stores/shortcuts";
import { toastsAtom } from "@/stores/toast";
import { openConflictsTabAction } from "@/stores/conflict";
import { useSetAtom, useAtomValue } from "jotai";
import { Textarea } from "@/components/ui/textarea";
import { DiffView } from "@/components/plan/DiffView";
import {
  GitBranch,
  Plus,
  Minus,
  ChevronUp,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Loader2,
  GitCommitHorizontal,
  List,
  Rocket,
  Upload,
  AlertTriangle,
  Check,
  CircleDashed,
  GitMerge,
  Maximize2,
} from "lucide-react";
import { Kbd } from "@/components/ui/kbd";
import { MergeModal } from "@/components/session/MergeModal";
import { activeSessionAtom, activeWorkspaceAtom, workspacesAtom, sessionTabIdsAtom, activeSessionIdAtom, type Workspace } from "@/stores/workspace";

interface ChangedFile {
  path: string;
  status: string;
}

interface GitFullStatus {
  staged: ChangedFile[];
  unstaged: ChangedFile[];
  untracked: string[];
}

interface CommitEntry {
  hash: string;
  message: string;
}

interface PrInfo {
  number: number;
  title: string;
  state: string;
  url: string;
}

const STATUS_COLORS: Record<string, string> = {
  Create: "text-green-400",
  Edit: "text-orange-400",
  Delete: "text-red-400",
  Rename: "text-blue-400",
};

const STATUS_LETTERS: Record<string, string> = {
  Create: "A",
  Edit: "M",
  Delete: "D",
  Rename: "R",
};

interface GitPanelProps {
  sessionId: string;
  hideHistory?: boolean;
  hideChanges?: boolean;
}

export function GitPanel({ sessionId, hideHistory = false, hideChanges = false }: GitPanelProps) {
  const [status, setStatus] = useState<GitFullStatus>({ staged: [], unstaged: [], untracked: [] });
  const [commits, setCommits] = useState<CommitEntry[]>([]);
  const [prInfo, setPrInfo] = useState<PrInfo | null>(null);
  const [branch, setBranch] = useState("");
  const [ahead, setAhead] = useState(0);
  const [commitMsg, setCommitMsg] = useState("");
  const [loading, setLoading] = useState(true);
  const [committing, setCommitting] = useState(false);
  const [creatingPr] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedFile, setExpandedFile] = useState<string | null>(null);
  const [expandedCommit, setExpandedCommit] = useState<string | null>(null);
  const [commitFiles, setCommitFiles] = useState<Record<string, string[]>>({});
  const [historyView, setHistoryView] = useState<"list" | "graph">("list");
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const conflictedFiles = useAtomValue(activeConflictedFilesAtom);
  const sessionsAutoMerged = useAtomValue(sessionsAutoMergedAtom);
  const setSessionsAutoMerged = useSetAtom(sessionsAutoMergedAtom);
  const setSessionTabIds = useSetAtom(sessionTabIdsAtom);
  const setActiveSessionId = useSetAtom(activeSessionIdAtom);
  const isAutoMergeConflict = sessionsAutoMerged.has(sessionId) && conflictedFiles.length > 0;
  const setPrChecksMap = useSetAtom(prChecksMapAtom);
  const openZenMode = useSetAtom(openZenModeAtom);
  const triggerShip = useSetAtom(triggerShipAtom);
  const triggerMergeSignal = useAtomValue(triggerMergeAtom);
  const addToast = useSetAtom(toastsAtom);
  const openConflictsTab = useSetAtom(openConflictsTabAction);
  const [ciChecks, setCiChecks] = useState<PrChecks | null>(null);
  const [pendingMerge, setPendingMerge] = useState(false);
  const [completing, setCompleting] = useState(false);
  const [mergeOpen, setMergeOpen] = useState(false);
  const activeSession = useAtomValue(activeSessionAtom);
  const activeWorkspace = useAtomValue(activeWorkspaceAtom);
  const setWorkspaces = useSetAtom(workspacesAtom);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const allFiles = [
    ...status.staged.map((f) => f.path),
    ...status.unstaged.map((f) => f.path),
    ...status.untracked,
  ];

  // Flat list for arrow-key navigation (Staged → Changes → Untracked).
  type NavEntry = { path: string; group: "staged" | "unstaged" | "untracked" };
  const flatNav: NavEntry[] = [
    ...status.staged.map((f) => ({ path: f.path, group: "staged" as const })),
    ...status.unstaged.map((f) => ({ path: f.path, group: "unstaged" as const })),
    ...status.untracked.map((p) => ({ path: p, group: "untracked" as const })),
  ];
  const [fileCursor, setFileCursor] = useState(0);
  useEffect(() => {
    if (fileCursor >= flatNav.length) setFileCursor(Math.max(0, flatNav.length - 1));
  }, [fileCursor, flatNav.length]);

  function openZen(filePath: string) {
    openZenMode({ filePath, sessionId, files: allFiles });
  }

  function toggleCommitExpand(hash: string) {
    if (expandedCommit === hash) {
      setExpandedCommit(null);
      return;
    }
    setExpandedCommit(hash);
    if (!commitFiles[hash]) {
      invoke<string[]>("get_commit_files", { sessionId, hash })
        .then((files) => setCommitFiles((prev) => ({ ...prev, [hash]: files })))
        .catch(() => {});
    }
  }

  function openCommitFileZen(hash: string, filePath: string) {
    const files = commitFiles[hash] ?? [filePath];
    openZenMode({ filePath, sessionId, files });
  }

  const refreshCore = useCallback(() => {
    Promise.all([
      invoke<GitFullStatus>("get_git_status", { sessionId }),
      invoke<{ branch: string; dirty: boolean; ahead: number }>("get_session_git_info", { sessionId }),
      invoke<CommitEntry[]>("get_recent_commits", { sessionId, count: 20 }),
    ])
      .then(([s, info, c]) => {
        setStatus(s);
        setBranch(info.branch);
        setAhead(info.ahead);
        setCommits(c);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [sessionId]);

  const refreshPr = useCallback(() => {
    invoke<PrInfo | null>("get_pr_status", { sessionId })
      .then(setPrInfo)
      .catch(() => setPrInfo(null));
  }, [sessionId]);

  const refreshPendingMerge = useCallback(() => {
    invoke<boolean>("has_pending_merge", { sessionId })
      .then(setPendingMerge)
      .catch(() => setPendingMerge(false));
  }, [sessionId]);

  useEffect(() => {
    refreshCore();
    refreshPr();
    refreshConflicts(sessionId);
    refreshPendingMerge();
    const unlisteners: (() => void)[] = [];
    listen("files:modified", () => { refreshCore(); refreshConflicts(sessionId); refreshPendingMerge(); }).then((fn) => unlisteners.push(fn));
    const id = setInterval(() => {
      if (!committing) { refreshCore(); refreshPr(); refreshPendingMerge(); }
    }, 10000);
    return () => { for (const fn of unlisteners) fn(); clearInterval(id); };
  }, [refreshCore, refreshPr, refreshConflicts, refreshPendingMerge, sessionId, committing]);

  useEffect(() => {
    if (pollRef.current) { clearInterval(pollRef.current); pollRef.current = null; }
    // Detect MERGED PRs but DO NOT auto-cleanup — we surface a banner
    // (rendered below) with an explicit "Cleanup session" button so the
    // destructive deletion is always user-triggered. This was reported as
    // a "session vanished without confirmation" bug; the banner is the fix.
    if (prInfo && prInfo.state !== "OPEN") { setCiChecks(null); return; }
    if (!prInfo) { setCiChecks(null); return; }
    const tick = () => {
      invoke<PrChecks | null>("poll_pr_checks", { sessionId })
        .then((result) => {
          setCiChecks(result);
          setPrChecksMap((prev) => ({ ...prev, [sessionId]: result }));
        })
        .catch(() => {});
    };
    tick();
    pollRef.current = setInterval(tick, 20000);
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [prInfo, sessionId, setPrChecksMap]);

  function handleStageFile(path: string) {
    invoke("git_stage_file", { sessionId, path }).then(refreshCore).catch(() => {});
  }

  function handleUnstageFile(path: string) {
    invoke("git_unstage_file", { sessionId, path }).then(refreshCore).catch(() => {});
  }

  function handleStageAll() {
    invoke("git_stage_all", { sessionId }).then(refreshCore).catch(() => {});
  }

  function handleUnstageAll() {
    invoke("git_unstage_all", { sessionId }).then(refreshCore).catch(() => {});
  }

  function handleCommit() {
    if (!commitMsg.trim() || status.staged.length === 0) return;
    setCommitting(true);
    invoke<string>("git_commit", { sessionId, message: commitMsg.trim() })
      .then(() => {
        setCommitMsg("");
        refreshGit(sessionId);
        refreshCore();
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setCommitting(false));
  }

  function handleShip(inlineMessage?: string | null) {
    triggerShip({ tick: Date.now(), sessionId, inlineMessage: inlineMessage ?? null });
  }

  function handlePush() {
    if (ahead === 0) {
      addToast({ message: "Push", description: "Nothing to push", type: "info" });
      return;
    }
    invoke("git_push", { sessionId })
      .then(() => { addToast({ message: "Push", description: "Pushed to remote", type: "success" }); refreshGit(sessionId); refreshCore(); })
      .catch((err: unknown) => addToast({ message: "Push failed", description: String(err), type: "error" }));
  }

  function handleOpenConflictTab(path: string) {
    openConflictsTab({ sessionId, path });
  }

  /// User-confirmed total cleanup (worktree + branch + plan files + DB row).
  /// Removes the session from the tab list and clears active selection so
  /// no ghost references remain after the underlying row disappears.
  async function handleCleanupSession() {
    try {
      const res = await invoke<{ deleted: boolean; warnings: string[] }>("cleanup_merged_session", { sessionId });
      addToast({
        message: "Session closed",
        description: res.warnings.length > 0
          ? `Cleanup ran with ${res.warnings.length} warning(s). Press Ctrl+N to start a new session.`
          : "Worktree, branch and plan files removed. Press Ctrl+N to start a new session.",
        type: "success",
      });
      setSessionsAutoMerged((prev) => {
        if (!prev.has(sessionId)) return prev;
        const next = new Set(prev);
        next.delete(sessionId);
        return next;
      });
      // Remove the deleted session from open tabs so Ctrl+Tab doesn't
      // navigate back to a ghost. If it was the active session, clear so
      // the next tab event picks a real one.
      setSessionTabIds((prev) => prev.filter((id) => id !== sessionId));
      setActiveSessionId((current) => (current === sessionId ? null : current));
      invoke<Workspace[]>("get_workspaces").then(setWorkspaces).catch(() => {});
    } catch (e) {
      console.error("cleanup_merged_session failed", e);
      addToast({ message: "Cleanup failed", description: String(e), type: "error" });
    }
  }

  async function handleCompleteMerge() {
    if (completing) return;
    setCompleting(true);
    try {
      await invoke<string>("complete_pending_merge", { sessionId });
      addToast({ message: "Merge", description: "Merge commit created", type: "success" });
      refreshPendingMerge();
      refreshCore();
      refreshGit(sessionId);
    } catch (e) {
      addToast({ message: "Merge failed", description: String(e), type: "error" });
    } finally {
      setCompleting(false);
    }
  }

  function toggleFileDiff(path: string) {
    setExpandedFile((prev) => (prev === path ? null : path));
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && e.shiftKey && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      const msg = commitMsg.trim();
      if (msg && status.staged.length > 0) {
        handleShip(msg);
      } else {
        handleShip(null);
      }
      return;
    }
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleCommit();
    }
  }

  // Arrow up/down navigates the flat files list; Space toggles stage/unstage
  // based on the section the cursor is in. Skipped when typing in the commit
  // textarea or any input/editor.
  useEffect(() => {
    if (flatNav.length === 0) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown") {
        e.preventDefault();
        setFileCursor((i) => (i + 1) % flatNav.length);
        return;
      }
      if (e.code === "ArrowUp") {
        e.preventDefault();
        setFileCursor((i) => (i - 1 + flatNav.length) % flatNav.length);
        return;
      }
      if (e.code === "Space") {
        e.preventDefault();
        const entry = flatNav[fileCursor];
        if (!entry) return;
        if (entry.group === "staged") handleUnstageFile(entry.path);
        else handleStageFile(entry.path);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flatNav.length, fileCursor]);

  // Ctrl+Shift+M opens the MergeModal — same dirty/ahead pre-check the
  // Sidebar used to do, now hosted here so this panel is the single owner.
  useEffect(() => {
    if (triggerMergeSignal === 0 || !activeSession) return;
    invoke<{ dirty: boolean; commits_ahead: boolean }>("check_session_has_commits", { sessionId: activeSession.id })
      .then((s) => {
        if (s.dirty) addToast({ message: "Merge", description: "Commit your changes first", type: "info" });
        else if (s.commits_ahead) setMergeOpen(true);
        else addToast({ message: "Merge", description: "Nothing to merge", type: "info" });
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [triggerMergeSignal]);

  useEffect(() => {
    function onOpenFirst(ev: Event) {
      const detail = (ev as CustomEvent<{ path: string }>).detail;
      if (detail?.path) handleOpenConflictTab(detail.path);
      else if (conflictedFiles[0]) handleOpenConflictTab(conflictedFiles[0]);
    }
    document.addEventListener("cluihud:open-first-conflict", onOpenFirst);
    return () => document.removeEventListener("cluihud:open-first-conflict", onOpenFirst);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conflictedFiles]);

  if (loading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <Loader2 size={16} className="animate-spin text-muted-foreground" />
        <span className="text-[11px] text-muted-foreground">Loading git status...</span>
      </div>
    );
  }

  const canCommit = status.staged.length > 0 && commitMsg.trim().length > 0 && !committing;
  const canCreatePr = ahead > 0 && !prInfo && !creatingPr;
  // Merge button: only show when the worktree actually has commits beyond
  // its base branch — merging a branch with no commits is a no-op and the
  // pre-check inside the trigger handler would just toast "Nothing to merge".
  // Hiding here matches the disable-when-nothing-to-do pattern used for
  // Push/Ship.
  const canMergeLocal = !!activeSession?.worktree_branch && ahead > 0;

  return (
    <div className="flex h-full flex-col">
      {/* Branch header */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <GitBranch size={12} className="text-muted-foreground" />
        <span className="text-[11px] font-medium text-foreground/80 font-mono truncate">{branch}</span>
        {ahead > 0 && (
          <span className="text-[10px] text-green-400">+{ahead} ahead</span>
        )}
        {prInfo && (
          <>
            <span className={`text-[10px] px-1.5 py-0.5 rounded ${
              prInfo.state === "OPEN" ? "bg-green-500/15 text-green-400" : "bg-purple-500/15 text-purple-400"
            }`}>
              PR #{prInfo.number}
            </span>
            {ciChecks && ciChecks.total > 0 && <CiBadge checks={ciChecks} />}
            <a href={prInfo.url} target="_blank" rel="noopener noreferrer" className="text-muted-foreground hover:text-foreground">
              <ExternalLink size={10} />
            </a>
          </>
        )}
        <button
          onClick={() => document.dispatchEvent(new CustomEvent("cluihud:expand-zen-git", { detail: { sessionId } }))}
          className="ml-auto flex h-5 items-center gap-1 rounded text-muted-foreground hover:text-foreground hover:bg-secondary px-1.5 transition-colors"
          title="Expand to Zen mode"
        >
          <Maximize2 size={10} />
          <Kbd keys="ctrl+shift+0" />
        </button>
      </div>

      {pendingMerge && conflictedFiles.length === 0 && (
        <div className="shrink-0 flex items-center gap-2 border-b border-border/50 bg-green-500/10 px-3 py-1.5">
          <Check size={12} className="text-green-400" />
          <span className="text-[11px] text-green-300">In-progress merge ready to finish (creates a local merge commit).</span>
          <button
            onClick={handleCompleteMerge}
            disabled={completing}
            className="ml-auto flex h-6 items-center gap-1.5 rounded bg-green-500/20 px-2 text-[10px] font-medium text-green-300 hover:bg-green-500/30 transition-colors disabled:opacity-50"
            title="Creates a merge commit locally. No push."
          >
            {completing ? <Loader2 size={10} className="animate-spin" /> : null}
            Finish merge <Kbd keys="ctrl+alt+enter" />
          </button>
        </div>
      )}

      {prInfo && prInfo.state !== "OPEN" && (
        <div className="shrink-0 flex items-center gap-2 border-b border-border/50 bg-purple-500/10 px-3 py-1.5">
          <Check size={12} className="text-purple-400" />
          <span className="text-[11px] text-purple-300">
            PR #{prInfo.number} is {prInfo.state.toLowerCase()}. Cleanup will delete this session, its worktree, branch and plan files.
          </span>
          <button
            onClick={handleCleanupSession}
            className="ml-auto flex h-6 items-center gap-1.5 rounded bg-purple-500/20 px-2 text-[10px] font-medium text-purple-300 hover:bg-purple-500/30 transition-colors"
            title="Permanently deletes the session — irreversible"
          >
            Cleanup session
          </button>
        </div>
      )}

      {isAutoMergeConflict && (
        <div className="shrink-0 flex items-center gap-2 border-b border-border/50 bg-yellow-500/10 px-3 py-1.5">
          <AlertTriangle size={12} className="text-yellow-400" />
          <span className="text-[11px] text-yellow-300">
            Auto-merge blocked by conflict — Conflicts panel pre-filled an Ask-Claude prompt; press <Kbd keys="ctrl+shift+r" /> to send.
          </span>
        </div>
      )}

      {conflictedFiles.length > 0 && (
        <div className="shrink-0 border-b border-border/50">
          <div className="flex items-center justify-between px-3 py-1">
            <span className="flex items-center gap-1 text-[10px] font-medium uppercase tracking-wider text-yellow-400">
              <AlertTriangle size={10} /> Conflicts ({conflictedFiles.length})
            </span>
          </div>
          {conflictedFiles.map((path) => {
            const name = path.split("/").pop() ?? path;
            return (
              <div key={path} className="group flex items-center gap-1 px-3 py-0.5 hover:bg-secondary/30 transition-colors">
                <span className="shrink-0 font-mono text-[10px] font-bold w-3 text-yellow-400">C</span>
                <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80" title={path}>{name}</span>
                <button
                  onClick={() => handleOpenConflictTab(path)}
                  className="flex h-5 shrink-0 items-center rounded bg-yellow-500/15 px-2 text-[10px] text-yellow-400 hover:bg-yellow-500/25 transition-colors"
                >
                  Resolve
                </button>
              </div>
            );
          })}
        </div>
      )}

      {error && (
        <div className="shrink-0 border-b border-border/50 px-3 py-1">
          <span className="text-[10px] text-red-400">{error}</span>
        </div>
      )}

      {/* Two-column: History (left) + Files sidebar (right) */}
      <div className="flex flex-1 overflow-hidden">
        {/* Left: History/Timeline */}
        {!hideHistory && (
        <div className={`overflow-y-auto ${hideChanges ? "flex-1" : "flex-1 border-r border-border/50"}`}>
          {commits.length > 0 ? (
            <div>
              <div className="flex items-center justify-between px-3 py-1.5">
                <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                  History ({commits.length})
                </span>
                <div className="flex items-center gap-0.5">
                  <button
                    onClick={() => setHistoryView("list")}
                    className={`rounded p-0.5 transition-colors ${historyView === "list" ? "text-foreground bg-secondary" : "text-muted-foreground hover:text-foreground"}`}
                    aria-label="List view"
                  >
                    <List size={10} />
                  </button>
                  <button
                    onClick={() => setHistoryView("graph")}
                    className={`rounded p-0.5 transition-colors ${historyView === "graph" ? "text-foreground bg-secondary" : "text-muted-foreground hover:text-foreground"}`}
                    aria-label="Graph view"
                  >
                    <GitCommitHorizontal size={10} />
                  </button>
                </div>
              </div>
              {commits.map((c, i) => {
                const isExpanded = expandedCommit === c.hash;
                const files = commitFiles[c.hash] ?? [];
                return (
                  <div key={c.hash}>
                    {historyView === "graph" ? (
                      <div
                        onClick={() => toggleCommitExpand(c.hash)}
                        className={`flex items-start gap-2 rounded px-3 py-0.5 transition-colors cursor-pointer ${isExpanded ? "bg-secondary/40" : "hover:bg-secondary/30"}`}
                      >
                        <div className="flex flex-col items-center pt-1">
                          <div className={`size-2 rounded-full ${isExpanded ? "bg-primary" : "bg-muted-foreground/60"}`} />
                          {i < commits.length - 1 && <div className="w-px flex-1 bg-border/50 min-h-3" />}
                        </div>
                        <div className="min-w-0 flex-1 pb-1">
                          <p className="truncate text-[10px] text-foreground/80">{c.message}</p>
                          <span className="font-mono text-[9px] text-muted-foreground/50">{c.hash}</span>
                        </div>
                      </div>
                    ) : (
                      <div
                        onClick={() => toggleCommitExpand(c.hash)}
                        className={`flex items-center gap-2 px-3 py-0.5 transition-colors cursor-pointer ${isExpanded ? "bg-secondary/40" : "hover:bg-secondary/30"}`}
                      >
                        <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">{c.hash}</span>
                        <span className="truncate text-[10px] text-foreground/70">{c.message}</span>
                      </div>
                    )}
                    {isExpanded && files.length > 0 && (
                      <div className="ml-6 mb-1">
                        {files.map((f) => {
                          const name = f.split("/").pop() ?? f;
                          return (
                            <button
                              key={f}
                              type="button"
                              onClick={() => openCommitFileZen(c.hash, f)}
                              className="flex w-full items-center gap-1.5 rounded px-2 py-0.5 text-left text-[10px] text-muted-foreground hover:bg-secondary/30 hover:text-foreground transition-colors"
                            >
                              <span className="size-1 shrink-0 rounded-full bg-muted-foreground/40" />
                              <span className="truncate">{name}</span>
                            </button>
                          );
                        })}
                      </div>
                    )}
                    {isExpanded && files.length === 0 && (
                      <div className="ml-6 mb-1 px-2 py-0.5">
                        <span className="text-[10px] text-muted-foreground/50">Loading files...</span>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex h-full items-center justify-center">
              <span className="text-[10px] text-muted-foreground">No commits yet</span>
            </div>
          )}
        </div>
        )}

        {/* Right sidebar: Staged / Unstaged / Untracked */}
        {!hideChanges && (
        <div className={`overflow-y-auto ${hideHistory ? "flex-1" : "w-52 shrink-0"}`}>
          <FileSection
            title="Staged"
            count={status.staged.length}
            action={status.staged.length > 0 ? { label: "Unstage all", icon: <ChevronDown size={10} />, onClick: handleUnstageAll } : undefined}
          >
            {status.staged.map((f, i) => (
              <FileRow
                key={f.path}
                path={f.path}
                status={f.status}
                actionIcon={<Minus size={10} />}
                actionLabel="Unstage"
                expanded={expandedFile === f.path}
                isCursor={fileCursor === i}
                onAction={() => handleUnstageFile(f.path)}
                onToggleDiff={() => toggleFileDiff(f.path)}
                onOpenZen={() => openZen(f.path)}
                onCursor={() => setFileCursor(i)}
                sessionId={sessionId}
              />
            ))}
          </FileSection>

          <FileSection
            title="Changes"
            count={status.unstaged.length}
            action={status.unstaged.length > 0 ? { label: "Stage all", icon: <ChevronUp size={10} />, onClick: handleStageAll } : undefined}
          >
            {status.unstaged.map((f, i) => {
              const navIdx = status.staged.length + i;
              return (
                <FileRow
                  key={f.path}
                  path={f.path}
                  status={f.status}
                  actionIcon={<Plus size={10} />}
                  actionLabel="Stage"
                  expanded={expandedFile === f.path}
                  isCursor={fileCursor === navIdx}
                  onAction={() => handleStageFile(f.path)}
                  onToggleDiff={() => toggleFileDiff(f.path)}
                  onOpenZen={() => openZen(f.path)}
                  onCursor={() => setFileCursor(navIdx)}
                  sessionId={sessionId}
                />
              );
            })}
          </FileSection>

          {status.untracked.length > 0 && (
            <FileSection title="Untracked" count={status.untracked.length}>
              {status.untracked.map((path, i) => {
                const navIdx = status.staged.length + status.unstaged.length + i;
                return (
                  <FileRow
                    key={path}
                    path={path}
                    status="Create"
                    actionIcon={<Plus size={10} />}
                    actionLabel="Stage"
                    expanded={expandedFile === path}
                    isCursor={fileCursor === navIdx}
                    onAction={() => handleStageFile(path)}
                    onToggleDiff={() => toggleFileDiff(path)}
                    onOpenZen={() => openZen(path)}
                    onCursor={() => setFileCursor(navIdx)}
                    sessionId={sessionId}
                  />
                );
              })}
            </FileSection>
          )}
          {flatNav.length > 0 && (
            <div className="sticky bottom-0 flex items-center gap-1 border-t border-border/40 bg-card/95 px-3 py-1 text-[9px] text-muted-foreground/60">
              <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> move · <Kbd keys="space" /> stage/unstage
            </div>
          )}
        </div>
        )}
      </div>

      {/* Fixed commit bar at bottom */}
      {!hideHistory && (
      <div className="shrink-0 border-t border-border/50 p-2">
        <Textarea
          value={commitMsg}
          onChange={(e) => setCommitMsg(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Commit message... (Ctrl+Enter = commit, Ctrl+Shift+Enter = ship)"
          className="mb-2 h-16 resize-none rounded border-border/50 bg-background font-mono text-[11px] leading-relaxed focus-visible:ring-1"
          spellCheck={false}
        />
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            onClick={handleCommit}
            disabled={!canCommit}
            className={`flex h-6 flex-1 items-center justify-center gap-1.5 rounded text-[11px] font-medium transition-colors ${
              canCommit
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-secondary text-muted-foreground cursor-not-allowed"
            }`}
          >
            {committing ? "Committing..." : `Commit (${status.staged.length})`}
            {canCommit && <Kbd keys="ctrl+enter" />}
          </button>
          {canCreatePr && (
            <button
              onClick={handlePush}
              className="flex h-6 items-center gap-1.5 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            >
              <Upload size={10} /> Push <Kbd keys="ctrl+alt+p" />
            </button>
          )}
          {(canCreatePr || creatingPr) && (
            <button
              onClick={() => handleShip(commitMsg.trim() || null)}
              disabled={creatingPr}
              className="flex h-6 items-center gap-1.5 rounded bg-green-500/15 px-2 text-[10px] font-medium text-green-400 hover:bg-green-500/25 transition-colors"
            >
              <Rocket size={10} /> Ship <Kbd keys="ctrl+shift+y" />
            </button>
          )}
          {canMergeLocal && (
            <button
              onClick={() => setMergeOpen(true)}
              className="flex h-6 items-center gap-1.5 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            >
              <GitMerge size={10} /> Merge
            </button>
          )}
        </div>
      </div>
      )}
      {activeSession && activeWorkspace && (
        <MergeModal
          open={mergeOpen}
          onOpenChange={setMergeOpen}
          session={activeSession}
          workspaceId={activeWorkspace.id}
          onMerged={() => {
            invoke<Workspace[]>("get_workspaces").then(setWorkspaces).catch(() => {});
            refreshGit(sessionId);
            refreshCore();
          }}
          onConflict={(_target, _detail) => {
            refreshConflicts(sessionId);
            refreshCore();
          }}
        />
      )}
    </div>
  );
}

function CiBadge({ checks }: { checks: PrChecks }) {
  if (checks.failing > 0) {
    return (
      <span title={`${checks.failing} failing / ${checks.total} total`} className="flex items-center gap-0.5 text-[10px] text-red-400">
        <AlertTriangle size={10} /> {checks.failing}/{checks.total}
      </span>
    );
  }
  if (checks.pending > 0) {
    return (
      <span title={`${checks.pending} pending`} className="flex items-center gap-0.5 text-[10px] text-yellow-400">
        <CircleDashed size={10} className="animate-spin" /> {checks.pending}
      </span>
    );
  }
  if (checks.passing > 0) {
    return (
      <span title={`${checks.passing} checks passing`} className="flex items-center gap-0.5 text-[10px] text-green-400">
        <Check size={10} /> {checks.passing}
      </span>
    );
  }
  return null;
}

function FileSection({
  title,
  count,
  action,
  children,
}: {
  title: string;
  count: number;
  action?: { label: string; icon: React.ReactNode; onClick: () => void };
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-border/50">
      <div className="flex items-center justify-between px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          {title} ({count})
        </span>
        {action && (
          <button
            onClick={action.onClick}
            className="flex items-center gap-0.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
          >
            {action.icon}
            {action.label}
          </button>
        )}
      </div>
      {children}
    </div>
  );
}

function FileRow({
  path,
  status,
  actionIcon,
  actionLabel,
  expanded,
  isCursor,
  onAction,
  onToggleDiff,
  onOpenZen,
  onCursor,
  sessionId,
}: {
  path: string;
  status: string;
  actionIcon: React.ReactNode;
  actionLabel: string;
  expanded: boolean;
  isCursor?: boolean;
  onAction: () => void;
  onToggleDiff: () => void;
  onOpenZen: () => void;
  onCursor?: () => void;
  sessionId: string;
}) {
  const filename = path.split("/").pop() ?? path;
  const color = STATUS_COLORS[status] ?? "text-muted-foreground";
  const letter = STATUS_LETTERS[status] ?? "?";

  return (
    <div>
      <div
        ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
        onMouseEnter={onCursor}
        className={`group flex items-center gap-1 px-3 py-0.5 transition-colors ${
          isCursor ? "bg-orange-500/15 border-l-2 border-l-orange-500" : "hover:bg-secondary/30 border-l-2 border-l-transparent"
        }`}
      >
        <button
          onClick={onToggleDiff}
          className="flex size-3 shrink-0 items-center justify-center text-muted-foreground/50"
        >
          <ChevronRight size={8} className={`transition-transform ${expanded ? "rotate-90" : ""}`} />
        </button>
        <span className={`shrink-0 font-mono text-[10px] font-bold w-3 ${color}`}>{letter}</span>
        <button
          onClick={onOpenZen}
          className="min-w-0 flex-1 truncate text-left text-[11px] text-foreground/80 hover:text-foreground transition-colors"
          title={path}
        >
          {filename}
        </button>
        <button
          onClick={onAction}
          className="flex size-4 shrink-0 items-center justify-center rounded opacity-0 transition-opacity group-hover:opacity-100 hover:bg-secondary text-muted-foreground hover:text-foreground"
          aria-label={actionLabel}
        >
          {actionIcon}
        </button>
      </div>
      {expanded && (
        <div className="mx-2 mb-1 max-h-64 overflow-hidden rounded border border-border/30">
          <DiffView filePath={path} sessionId={sessionId} onOpenZen={onOpenZen} />
        </div>
      )}
    </div>
  );
}
