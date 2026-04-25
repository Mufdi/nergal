import { useState, useEffect, useCallback, useRef } from "react";
import { invoke, listen } from "@/lib/tauri";
import { refreshGitInfoAtom, refreshConflictedFilesAtom, activeConflictedFilesAtom, prChecksMapAtom, type PrChecks } from "@/stores/git";
import { openZenModeAtom } from "@/stores/zenMode";
import { triggerShipAtom } from "@/stores/ship";
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
} from "lucide-react";

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
  const setPrChecksMap = useSetAtom(prChecksMapAtom);
  const openZenMode = useSetAtom(openZenModeAtom);
  const triggerShip = useSetAtom(triggerShipAtom);
  const addToast = useSetAtom(toastsAtom);
  const openConflictsTab = useSetAtom(openConflictsTabAction);
  const [ciChecks, setCiChecks] = useState<PrChecks | null>(null);
  const [pendingMerge, setPendingMerge] = useState(false);
  const [completing, setCompleting] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const allFiles = [
    ...status.staged.map((f) => f.path),
    ...status.unstaged.map((f) => f.path),
    ...status.untracked,
  ];

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
    if (!prInfo || prInfo.state !== "OPEN") { setCiChecks(null); return; }
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
  const showShipBadge = ahead > 0 && !prInfo && !committing;

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
        {showShipBadge && (
          <div className="ml-auto flex items-center gap-1">
            <button
              onClick={() => handleShip(null)}
              className="flex h-5 items-center gap-1 rounded bg-green-500/15 px-2 text-[10px] font-medium text-green-400 hover:bg-green-500/25 transition-colors"
              title="Ship (Ctrl+Shift+Y)"
            >
              <Rocket size={10} />
              Ship it
            </button>
            <button
              onClick={handlePush}
              className="flex h-5 items-center gap-1 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
              title="Push (Ctrl+Shift+U)"
            >
              <Upload size={10} />
              Push
            </button>
          </div>
        )}
      </div>

      {pendingMerge && conflictedFiles.length === 0 && (
        <div className="shrink-0 flex items-center gap-2 border-b border-border/50 bg-green-500/10 px-3 py-1.5">
          <Check size={12} className="text-green-400" />
          <span className="text-[11px] text-green-300">All conflicts resolved — ready to finish merge.</span>
          <button
            onClick={handleCompleteMerge}
            disabled={completing}
            className="ml-auto flex h-6 items-center gap-1 rounded bg-green-500/20 px-2 text-[10px] font-medium text-green-300 hover:bg-green-500/30 transition-colors disabled:opacity-50"
          >
            {completing ? <Loader2 size={10} className="animate-spin" /> : null}
            Complete merge
          </button>
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
            {status.staged.map((f) => (
              <FileRow
                key={f.path}
                path={f.path}
                status={f.status}
                actionIcon={<Minus size={10} />}
                actionLabel="Unstage"
                expanded={expandedFile === f.path}
                onAction={() => handleUnstageFile(f.path)}
                onToggleDiff={() => toggleFileDiff(f.path)}
                onOpenZen={() => openZen(f.path)}
                sessionId={sessionId}
              />
            ))}
          </FileSection>

          <FileSection
            title="Changes"
            count={status.unstaged.length}
            action={status.unstaged.length > 0 ? { label: "Stage all", icon: <ChevronUp size={10} />, onClick: handleStageAll } : undefined}
          >
            {status.unstaged.map((f) => (
              <FileRow
                key={f.path}
                path={f.path}
                status={f.status}
                actionIcon={<Plus size={10} />}
                actionLabel="Stage"
                expanded={expandedFile === f.path}
                onAction={() => handleStageFile(f.path)}
                onToggleDiff={() => toggleFileDiff(f.path)}
                onOpenZen={() => openZen(f.path)}
                sessionId={sessionId}
              />
            ))}
          </FileSection>

          {status.untracked.length > 0 && (
            <FileSection title="Untracked" count={status.untracked.length}>
              {status.untracked.map((path) => (
                <FileRow
                  key={path}
                  path={path}
                  status="Create"
                  actionIcon={<Plus size={10} />}
                  actionLabel="Stage"
                  expanded={expandedFile === path}
                  onAction={() => handleStageFile(path)}
                  onToggleDiff={() => toggleFileDiff(path)}
                  onOpenZen={() => openZen(path)}
                  sessionId={sessionId}
                />
              ))}
            </FileSection>
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
        <div className="flex gap-1.5">
          <button
            onClick={handleCommit}
            disabled={!canCommit}
            className={`flex h-6 flex-1 items-center justify-center gap-1 rounded text-[11px] font-medium transition-colors ${
              canCommit
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-secondary text-muted-foreground cursor-not-allowed"
            }`}
          >
            {committing ? "Committing..." : `Commit (${status.staged.length})`}
          </button>
          {canCreatePr && (
            <button
              onClick={handlePush}
              className="flex h-6 items-center gap-1 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
              title="Push only (Ctrl+Shift+U)"
            >
              <Upload size={10} /> Push
            </button>
          )}
          {(canCreatePr || creatingPr) && (
            <button
              onClick={() => handleShip(commitMsg.trim() || null)}
              disabled={creatingPr}
              className="flex h-6 items-center gap-1 rounded bg-green-500/15 px-2 text-[10px] font-medium text-green-400 hover:bg-green-500/25 transition-colors"
              title="Ship: commit + push + PR (Ctrl+Shift+Y)"
            >
              <Rocket size={10} /> Ship
            </button>
          )}
        </div>
      </div>
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
  onAction,
  onToggleDiff,
  onOpenZen,
  sessionId,
}: {
  path: string;
  status: string;
  actionIcon: React.ReactNode;
  actionLabel: string;
  expanded: boolean;
  onAction: () => void;
  onToggleDiff: () => void;
  onOpenZen: () => void;
  sessionId: string;
}) {
  const filename = path.split("/").pop() ?? path;
  const color = STATUS_COLORS[status] ?? "text-muted-foreground";
  const letter = STATUS_LETTERS[status] ?? "?";

  return (
    <div>
      <div className="group flex items-center gap-1 px-3 py-0.5 hover:bg-secondary/30 transition-colors">
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
