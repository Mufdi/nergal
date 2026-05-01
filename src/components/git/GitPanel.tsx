import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke, listen } from "@/lib/tauri";
import {
  refreshGitInfoAtom,
  refreshConflictedFilesAtom,
  activeConflictedFilesAtom,
  prChecksMapAtom,
  transitionAfterCleanupAction,
  gitChipModeAtom,
  type ChipMode,
  type PrChecks,
  type PrSummary,
} from "@/stores/git";
import { triggerMergeAtom } from "@/stores/shortcuts";
import { toastsAtom } from "@/stores/toast";
import { selectedConflictFileMapAtom } from "@/stores/conflict";
import { useSetAtom, useAtomValue } from "jotai";
import {
  GitBranch,
  ExternalLink,
  Loader2,
  AlertTriangle,
  Check,
  CircleDashed,
  Maximize2,
} from "lucide-react";
import { Kbd } from "@/components/ui/kbd";
import { workspacesAtom } from "@/stores/workspace";
import { ChipStrip } from "./chips/ChipStrip";
import { FilesChip } from "./chips/FilesChip";
import { HistoryChip } from "./chips/HistoryChip";
import { StashesChip } from "./chips/StashesChip";
import { PrsChip } from "./chips/PrsChip";
import { ConflictsChip } from "./chips/ConflictsChip";

interface PrInfo {
  number: number;
  title: string;
  state: string;
  url: string;
}

interface GitPanelProps {
  sessionId: string;
}

export function GitPanel({ sessionId }: GitPanelProps) {
  const [prInfo, setPrInfo] = useState<PrInfo | null>(null);
  const [branch, setBranch] = useState("");
  const [ahead, setAhead] = useState(0);
  const [loading, setLoading] = useState(true);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const conflictedFiles = useAtomValue(activeConflictedFilesAtom);
  const transitionAfterCleanup = useSetAtom(transitionAfterCleanupAction);
  const setPrChecksMap = useSetAtom(prChecksMapAtom);
  const triggerMergeSignal = useAtomValue(triggerMergeAtom);
  const addToast = useSetAtom(toastsAtom);
  const setSelectedConflictMap = useSetAtom(selectedConflictFileMapAtom);
  const [ciChecks, setCiChecks] = useState<PrChecks | null>(null);
  const [pendingMerge, setPendingMerge] = useState(false);
  const [completing, setCompleting] = useState(false);
  const workspaces = useAtomValue(workspacesAtom);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const chipModeMap = useAtomValue(gitChipModeAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);
  const [prCount, setPrCount] = useState(0);
  const [stashCount, setStashCount] = useState(0);

  const workspaceId = useMemo<string | null>(() => {
    for (const ws of workspaces) {
      if (ws.sessions.some((s) => s.id === sessionId)) return ws.id;
    }
    return null;
  }, [workspaces, sessionId]);

  const chipMode: ChipMode = workspaceId ? (chipModeMap[workspaceId] ?? "files") : "files";

  const setChipMode = useCallback((mode: ChipMode) => {
    if (!workspaceId) return;
    setChipModeMap((prev) => ({ ...prev, [workspaceId]: mode }));
  }, [workspaceId, setChipModeMap]);

  const refreshHeader = useCallback(() => {
    Promise.all([
      invoke<{ branch: string; dirty: boolean; ahead: number }>("get_session_git_info", { sessionId }),
    ])
      .then(([info]) => {
        setBranch(info.branch);
        setAhead(info.ahead);
      })
      .catch(() => {})
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

  const refreshChipCounts = useCallback(async () => {
    if (workspaceId) {
      try {
        const prs = await invoke<PrSummary[]>("list_prs", { workspaceId });
        setPrCount(prs.filter((p) => p.state === "OPEN").length);
      } catch {
        setPrCount(0);
      }
    }
    try {
      const stashes = await invoke<{ index: number }[]>("git_stash_list", { sessionId });
      setStashCount(stashes.length);
    } catch {
      setStashCount(0);
    }
  }, [workspaceId, sessionId]);

  useEffect(() => {
    refreshHeader();
    refreshPr();
    refreshConflicts(sessionId);
    refreshPendingMerge();
    refreshChipCounts();
    const unlisteners: (() => void)[] = [];
    listen("files:modified", () => { refreshHeader(); refreshConflicts(sessionId); refreshPendingMerge(); refreshChipCounts(); })
      .then((fn) => unlisteners.push(fn));
    const id = setInterval(() => { refreshHeader(); refreshPr(); refreshPendingMerge(); refreshChipCounts(); }, 15000);
    return () => { for (const fn of unlisteners) fn(); clearInterval(id); };
  }, [refreshHeader, refreshPr, refreshConflicts, refreshPendingMerge, refreshChipCounts, sessionId]);

  useEffect(() => {
    if (pollRef.current) { clearInterval(pollRef.current); pollRef.current = null; }
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

  /// Routes the user to the Conflicts chip with the given file pre-selected.
  /// Replaces the legacy `openConflictsTab` flow now that conflicts live in
  /// a chip rather than a document tab.
  function handleRouteToConflictChip(path: string) {
    setSelectedConflictMap((prev) => ({ ...prev, [sessionId]: path }));
    setChipMode("conflicts");
  }

  async function handleCleanupSession() {
    if (!workspaceId) {
      addToast({ message: "Cleanup failed", description: "No workspace bound to this session", type: "error" });
      return;
    }
    try {
      const res = await invoke<{ deleted: boolean; warnings: string[]; archived_plans_path: string | null }>(
        "cleanup_merged_session",
        { sessionId },
      );
      await transitionAfterCleanup({
        deletedSessionId: sessionId,
        workspaceId,
        warnings: res.warnings,
        archivedPlansPath: res.archived_plans_path,
      });
    } catch (e) {
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
      refreshGit(sessionId);
    } catch (e) {
      addToast({ message: "Merge failed", description: String(e), type: "error" });
    } finally {
      setCompleting(false);
    }
  }

  void triggerMergeSignal;

  useEffect(() => {
    function onOpenFirst(ev: Event) {
      const detail = (ev as CustomEvent<{ path: string }>).detail;
      if (detail?.path) handleRouteToConflictChip(detail.path);
      else if (conflictedFiles[0]) handleRouteToConflictChip(conflictedFiles[0]);
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

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <GitBranch size={12} className="text-muted-foreground" />
        <span className="text-[11px] font-medium text-foreground/80 font-mono truncate">{branch}</span>
        {ahead > 0 && <span className="text-[10px] text-green-400">+{ahead} ahead</span>}
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

      <ChipStrip
        active={chipMode}
        conflictCount={conflictedFiles.length}
        prCount={prCount}
        stashCount={stashCount}
        onSelect={setChipMode}
      />

      <div className="flex-1 overflow-hidden">
        {chipMode === "files" && <FilesChip sessionId={sessionId} ahead={ahead} />}
        {chipMode === "history" && <HistoryChip sessionId={sessionId} />}
        {chipMode === "stashes" && <StashesChip sessionId={sessionId} />}
        {chipMode === "prs" && <PrsChip sessionId={sessionId} workspaceId={workspaceId} />}
        {chipMode === "conflicts" && <ConflictsChip sessionId={sessionId} />}
      </div>
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
