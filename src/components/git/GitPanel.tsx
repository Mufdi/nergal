import { useEffect, useCallback, useRef, useState } from "react";
import { invoke, listen } from "@/lib/tauri";
import {
  refreshGitInfoAtom,
  refreshConflictedFilesAtom,
  activeConflictedFilesAtom,
  prChecksMapAtom,
  prsCacheMapAtom,
  stashCountMapAtom,
  gitHeaderMapAtom,
  prInfoMapAtom,
  pendingMergeMapAtom,
  PRS_CACHE_TTL_MS,
  STASH_CACHE_TTL_MS,
  SESSION_GIT_TTL_MS,
  PR_INFO_TTL_MS,
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
  AlertTriangle,
  Check,
  CircleDashed,
  Loader2,
  Maximize2,
} from "lucide-react";
import { Kbd } from "@/components/ui/kbd";
import { sessionToWorkspaceMapAtom } from "@/stores/workspace";
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
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const refreshConflicts = useSetAtom(refreshConflictedFilesAtom);
  const conflictedFiles = useAtomValue(activeConflictedFilesAtom);
  const transitionAfterCleanup = useSetAtom(transitionAfterCleanupAction);
  const setPrChecksMap = useSetAtom(prChecksMapAtom);
  const triggerMergeSignal = useAtomValue(triggerMergeAtom);
  const addToast = useSetAtom(toastsAtom);
  const setSelectedConflictMap = useSetAtom(selectedConflictFileMapAtom);
  const [ciChecks, setCiChecks] = useState<PrChecks | null>(null);
  const [completing, setCompleting] = useState(false);
  const sessionToWorkspace = useAtomValue(sessionToWorkspaceMapAtom);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const chipModeMap = useAtomValue(gitChipModeAtom);
  const setChipModeMap = useSetAtom(gitChipModeAtom);
  const prsCacheMap = useAtomValue(prsCacheMapAtom);
  const setPrsCacheMap = useSetAtom(prsCacheMapAtom);
  const stashCountMap = useAtomValue(stashCountMapAtom);
  const setStashCountMap = useSetAtom(stashCountMapAtom);
  const gitHeaderMap = useAtomValue(gitHeaderMapAtom);
  const setGitHeaderMap = useSetAtom(gitHeaderMapAtom);
  const prInfoMap = useAtomValue(prInfoMapAtom);
  const setPrInfoMap = useSetAtom(prInfoMapAtom);
  const pendingMergeMap = useAtomValue(pendingMergeMapAtom);
  const setPendingMergeMap = useSetAtom(pendingMergeMapAtom);

  const workspaceId: string | null = sessionToWorkspace[sessionId] ?? null;
  const chipMode: ChipMode = chipModeMap[sessionId] ?? "files";

  /// All header + counts derive from caches: switching to a session that's
  /// been visited paints the panel instantly with the last-known values.
  /// Background fetches refresh in place — no spinner, no flash.
  const headerEntry = gitHeaderMap[sessionId];
  const branch = headerEntry?.branch ?? "";
  const ahead = headerEntry?.ahead ?? 0;
  const prInfo: PrInfo | null = prInfoMap[sessionId]?.data ?? null;
  const pendingMerge = pendingMergeMap[sessionId]?.pending ?? false;
  const prCount = workspaceId
    ? (prsCacheMap[workspaceId]?.data.filter((p) => p.state === "OPEN").length ?? 0)
    : 0;
  const stashCount = stashCountMap[sessionId]?.count ?? 0;

  const setChipMode = useCallback((mode: ChipMode) => {
    setChipModeMap((prev) => ({ ...prev, [sessionId]: mode }));
  }, [sessionId, setChipModeMap]);

  const refreshHeader = useCallback(() => {
    invoke<{ branch: string; dirty: boolean; ahead: number }>("get_session_git_info", { sessionId })
      .then((info) => {
        setGitHeaderMap((prev) => ({ ...prev, [sessionId]: { branch: info.branch, ahead: info.ahead, fetchedAt: Date.now() } }));
      })
      .catch(() => {});
  }, [sessionId, setGitHeaderMap]);

  const refreshPr = useCallback(() => {
    invoke<PrInfo | null>("get_pr_status", { sessionId })
      .then((data) => {
        setPrInfoMap((prev) => ({ ...prev, [sessionId]: { data, fetchedAt: Date.now() } }));
      })
      .catch(() => {
        setPrInfoMap((prev) => ({ ...prev, [sessionId]: { data: null, fetchedAt: Date.now() } }));
      });
  }, [sessionId, setPrInfoMap]);

  const refreshPendingMerge = useCallback(() => {
    invoke<boolean>("has_pending_merge", { sessionId })
      .then((pending) => {
        setPendingMergeMap((prev) => ({ ...prev, [sessionId]: { pending, fetchedAt: Date.now() } }));
      })
      .catch(() => {});
  }, [sessionId, setPendingMergeMap]);

  const refreshStashCount = useCallback(() => {
    invoke<{ index: number }[]>("git_stash_list", { sessionId })
      .then((stashes) => {
        setStashCountMap((prev) => ({ ...prev, [sessionId]: { count: stashes.length, fetchedAt: Date.now() } }));
      })
      .catch(() => {});
  }, [sessionId, setStashCountMap]);

  const refreshPrsList = useCallback((wsId: string) => {
    invoke<PrSummary[]>("list_prs", { workspaceId: wsId })
      .then((rows) => {
        setPrsCacheMap((prev) => ({ ...prev, [wsId]: { data: rows, fetchedAt: Date.now() } }));
      })
      .catch(() => {});
  }, [setPrsCacheMap]);

  /// Session-scoped fetches: per-session git/PR status + stash count. All
  /// cache-gated on mount — switching to a session whose cache is still
  /// fresh skips the round-trip entirely. Background polling (5s) and the
  /// `files:modified` listener keep caches warm. Conflicts always refresh
  /// on mount because they drive a critical UI signal (red ring on the
  /// Conflicts chip) and are cheap.
  useEffect(() => {
    const now = Date.now();
    const headerEntry = gitHeaderMap[sessionId];
    if (!headerEntry || now - headerEntry.fetchedAt > SESSION_GIT_TTL_MS) refreshHeader();
    const prEntry = prInfoMap[sessionId];
    if (!prEntry || now - prEntry.fetchedAt > PR_INFO_TTL_MS) refreshPr();
    const mergeEntry = pendingMergeMap[sessionId];
    if (!mergeEntry || now - mergeEntry.fetchedAt > SESSION_GIT_TTL_MS) refreshPendingMerge();
    const stashEntry = stashCountMap[sessionId];
    if (!stashEntry || now - stashEntry.fetchedAt > STASH_CACHE_TTL_MS) refreshStashCount();
    refreshConflicts(sessionId);

    const unlisteners: (() => void)[] = [];
    listen("files:modified", () => {
      refreshHeader();
      refreshConflicts(sessionId);
      refreshPendingMerge();
      refreshStashCount();
    }).then((fn) => unlisteners.push(fn));
    const id = setInterval(() => {
      refreshHeader();
      refreshPr();
      refreshPendingMerge();
    }, 5000);
    return () => { for (const fn of unlisteners) fn(); clearInterval(id); };
    // Cache atoms intentionally omitted from deps: read for the staleness
    // gate but the effect must not re-run on every cache mutation (would
    // create a refresh loop).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshHeader, refreshPr, refreshConflicts, refreshPendingMerge, refreshStashCount, sessionId]);

  /// Workspace-scoped fetch: `gh pr list` is a network round-trip, so it's
  /// keyed by workspaceId (not sessionId), cached for PRS_CACHE_TTL_MS, and
  /// polled at a relaxed 60s. Switching session within the same workspace
  /// reads the cache directly — no spinner, no network.
  useEffect(() => {
    if (!workspaceId) return;
    const cached = prsCacheMap[workspaceId];
    if (!cached || Date.now() - cached.fetchedAt > PRS_CACHE_TTL_MS) {
      refreshPrsList(workspaceId);
    }
    const id = setInterval(() => refreshPrsList(workspaceId), 60_000);
    return () => clearInterval(id);
    // prsCacheMap intentionally omitted: same reason as above — we read it
    // for the staleness gate but the effect must not re-run on every cache
    // mutation (would create a refresh loop).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, refreshPrsList]);

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

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-3 py-1.5">
        <GitBranch size={12} className="text-muted-foreground" />
        <span className="text-[11px] font-medium text-foreground/80 font-mono truncate">
          {branch || <span className="text-muted-foreground/40">…</span>}
        </span>
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
