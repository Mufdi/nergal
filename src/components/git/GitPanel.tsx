import { useState, useEffect, useCallback } from "react";
import { invoke, listen } from "@/lib/tauri";
import { refreshGitInfoAtom } from "@/stores/git";
import { openZenModeAtom } from "@/stores/zenMode";
import { useSetAtom } from "jotai";
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
  Maximize2,
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
  const [creatingPr, setCreatingPr] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedFile, setExpandedFile] = useState<string | null>(null);
  const [expandedCommit, setExpandedCommit] = useState<string | null>(null);
  const [commitFiles, setCommitFiles] = useState<Record<string, string[]>>({});
  const [historyView, setHistoryView] = useState<"list" | "graph">("list");
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const openZenMode = useSetAtom(openZenModeAtom);

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

  useEffect(() => {
    refreshCore();
    refreshPr();
    const unlisteners: (() => void)[] = [];
    listen("files:modified", () => refreshCore()).then((fn) => unlisteners.push(fn));
    return () => { for (const fn of unlisteners) fn(); };
  }, [refreshCore, refreshPr]);

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

  function handleCreatePr() {
    const title = commitMsg.trim() || branch;
    setCreatingPr(true);
    invoke<PrInfo>("create_pr", { sessionId, title, body: "" })
      .then((pr) => setPrInfo(pr))
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setCreatingPr(false));
  }

  function toggleFileDiff(path: string) {
    setExpandedFile((prev) => (prev === path ? null : path));
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleCommit();
    }
  }

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
            <a href={prInfo.url} target="_blank" rel="noopener noreferrer" className="text-muted-foreground hover:text-foreground">
              <ExternalLink size={10} />
            </a>
          </>
        )}
      </div>

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
          placeholder="Commit message... (Ctrl+Enter)"
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
          {(canCreatePr || creatingPr) && (
            <button
              onClick={handleCreatePr}
              disabled={creatingPr}
              className="flex h-6 items-center gap-1 rounded border border-border/50 px-2 text-[10px] text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            >
              {creatingPr ? "Creating..." : "Create PR"}
            </button>
          )}
        </div>
      </div>
      )}
    </div>
  );
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
        <div className="relative mx-2 mb-1 max-h-64 overflow-hidden rounded border border-border/30">
          <DiffView filePath={path} sessionId={sessionId} />
          <button
            onClick={onOpenZen}
            className="absolute right-1 top-1 flex size-5 items-center justify-center rounded bg-card/80 text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
            aria-label="Expand to Zen Mode"
          >
            <Maximize2 size={10} />
          </button>
        </div>
      )}
    </div>
  );
}
