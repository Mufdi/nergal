import { useState, useEffect, useCallback, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { GitBranch, RefreshCw } from "lucide-react";
import { invoke } from "@/lib/tauri";
import { loadLocalBranches } from "@/stores/localBranchesLoader";
import { confirm } from "@/lib/confirm";
import {
  branchActionsAllowed,
  branchDeleteToast,
  deleteBranchConfirm,
  orphanBranchCount,
  type LocalBranchDelete,
  type LocalBranchRow,
} from "@/lib/localBranches";
import {
  branchKey,
  branchTreeItems,
  branchTreeKeyAction,
  changedBranches,
  cursorIndexFor,
  openSessionConfirm,
  type BranchCommit,
} from "@/lib/branchTree";
import { chipKeysActive } from "@/lib/gitChipKeys";
import { localBranchesCacheAtom } from "@/stores/git";
import { openZenModeAtom, zenModeAtom, prZenAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import { addAndActivateSessionAtom } from "@/stores/sessionActivation";
import { focusZoneAtom } from "@/stores/shortcuts";
import { toastsAtom } from "@/stores/toast";
import { workspacesAtom, type Session } from "@/stores/workspace";
import * as terminalService from "@/components/terminal/terminalService";
import { Kbd } from "@/components/ui/kbd";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

interface BranchesChipProps {
  sessionId: string;
  workspaceId: string | null;
}

function relativeDate(unix: number): string {
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unix);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}d ago`;
  if (diff < 86400 * 365) return `${Math.floor(diff / (86400 * 30))}mo ago`;
  return `${Math.floor(diff / (86400 * 365))}y ago`;
}

const ACTION_BTN =
  "flex h-5 items-center rounded px-2 text-[10px] transition-colors focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary";

export function BranchesChip({ sessionId, workspaceId }: BranchesChipProps) {
  const cache = useAtomValue(localBranchesCacheAtom);
  const setCache = useSetAtom(localBranchesCacheAtom);
  const rows: LocalBranchRow[] = workspaceId ? (cache[workspaceId] ?? []) : [];
  const loaded = workspaceId ? cache[workspaceId] !== undefined : true;
  const [refreshing, setRefreshing] = useState(false);
  const [cursorKey, setCursorKey] = useState<string | null>(null);
  const lastCursorIdx = useRef(0);
  const [expandedBranch, setExpandedBranch] = useState<string | null>(null);
  const [expandedCommit, setExpandedCommit] = useState<string | null>(null);
  const [branchCommits, setBranchCommits] = useState<Record<string, BranchCommit[]>>({});
  const [commitFiles, setCommitFiles] = useState<Record<string, string[]>>({});
  const [busyBranch, setBusyBranch] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const rowsRef = useRef(rows);
  rowsRef.current = rows;

  const openZenMode = useSetAtom(openZenModeAtom);
  const addAndActivateSession = useSetAtom(addAndActivateSessionAtom);
  const setFocusZone = useSetAtom(focusZoneAtom);
  const focusZone = useAtomValue(focusZoneAtom);
  const addToast = useSetAtom(toastsAtom);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const workspaces = useAtomValue(workspacesAtom);
  const sessionIdsKey = (workspaces.find((w) => w.id === workspaceId)?.sessions ?? [])
    .map((s) => s.id)
    .join(",");
  const listenerActive = chipKeysActive({
    inZen: false,
    anyZenOpen: zenState.open || conflictsZen || prZen !== null,
    zenZone: "",
    focusZone,
  });

  const items = branchTreeItems(rows, { expandedBranch, expandedCommit, branchCommits, commitFiles });
  const cursor = cursorIndexFor(items, cursorKey, lastCursorIdx.current);
  lastCursorIdx.current = cursor;
  const cursorItemKey = items[cursor]?.key ?? null;

  const reload = useCallback(async (opts: { fresh?: boolean; silent?: boolean } = {}) => {
    if (!workspaceId) return;
    if (!opts.silent) setRefreshing(true);
    try {
      const next = await loadLocalBranches(workspaceId, { fresh: opts.fresh ?? true });
      const stale = changedBranches(rowsRef.current, next);
      setLoadError(null);
      setCache((prev) => (JSON.stringify(prev[workspaceId]) === JSON.stringify(next) ? prev : { ...prev, [workspaceId]: next }));
      if (stale.length > 0) {
        setBranchCommits((prev) => {
          const kept = { ...prev };
          for (const name of stale) delete kept[name];
          return kept;
        });
      }
      setExpandedBranch((b) => (b !== null && next.some((r) => r.name === b) ? b : null));
    } catch (e) {
      if (opts.silent) return;
      setLoadError(String(e));
      addToast({ message: "Could not load branches", description: String(e), type: "error" });
    } finally {
      if (!opts.silent) setRefreshing(false);
    }
  }, [workspaceId, setCache, addToast]);

  useEffect(() => {
    setLoadError(null);
    void reload({ fresh: false });
  }, [reload]);

  // Branches also change from a terminal (`git branch`, `git push`), which
  // emits no event Nergal can observe.
  useEffect(() => {
    const id = setInterval(() => void reload({ silent: true }), 5000);
    return () => clearInterval(id);
  }, [reload]);

  const firstSessionsKey = useRef(sessionIdsKey);
  useEffect(() => {
    if (firstSessionsKey.current === sessionIdsKey) return;
    firstSessionsKey.current = sessionIdsKey;
    void reload({ silent: true });
  }, [sessionIdsKey, reload]);

  useEffect(() => {
    if (!expandedBranch || !workspaceId || branchCommits[expandedBranch]) return;
    const branch = expandedBranch;
    invoke<BranchCommit[]>("get_branch_commits", { workspaceId, branch })
      .then((commits) => setBranchCommits((prev) => ({ ...prev, [branch]: commits })))
      .catch(() => setBranchCommits((prev) => ({ ...prev, [branch]: [] })));
  }, [expandedBranch, branchCommits, workspaceId]);

  function toggleBranch(branch: string) {
    setExpandedCommit(null);
    if (expandedBranch === branch) {
      setExpandedBranch(null);
      setCursorKey(branchKey(branch));
      return;
    }
    setExpandedBranch(branch);
  }

  function toggleCommit(branch: string, hash: string) {
    if (expandedCommit === hash) {
      setExpandedCommit(null);
      setCursorKey(`c:${branch}:${hash}`);
      return;
    }
    setExpandedCommit(hash);
    if (!workspaceId || commitFiles[hash]) return;
    invoke<string[]>("get_scoped_commit_files", {
      scope: { kind: "workspace", workspaceId },
      hash,
    })
      .then((files) => setCommitFiles((prev) => ({ ...prev, [hash]: files })))
      .catch(() => setCommitFiles((prev) => ({ ...prev, [hash]: [] })));
  }

  function openCommitFileZen(hash: string, filePath: string) {
    if (!workspaceId) return;
    openZenMode({
      filePath,
      sessionId,
      files: commitFiles[hash] ?? [filePath],
      commit: { hash, scope: { kind: "workspace", workspaceId } },
    });
  }

  async function openSession(row: LocalBranchRow) {
    if (!workspaceId || busyBranch) return;
    if (!(await confirm(openSessionConfirm(row)))) return;
    setBusyBranch(row.name);
    try {
      const session = await invoke<Session>("open_branch_session", { workspaceId, branch: row.name });
      addAndActivateSession(session);
      setFocusZone("terminal");
      requestAnimationFrame(() => terminalService.focusActive());
    } catch (e) {
      addToast({ message: "Could not open session", description: String(e), type: "error" });
    } finally {
      setBusyBranch(null);
      void reload();
    }
  }

  async function deleteBranch(row: LocalBranchRow) {
    if (!workspaceId || busyBranch) return;
    if (!(await confirm(deleteBranchConfirm(row)))) return;
    setBusyBranch(row.name);
    try {
      const result = await invoke<LocalBranchDelete>("delete_local_branch", {
        workspaceId,
        branch: row.name,
        expectedSha: row.sha,
      });
      addToast(branchDeleteToast(row.name, result));
    } catch (e) {
      addToast({ message: "Could not delete branch", description: String(e), type: "error" });
    } finally {
      setBusyBranch(null);
      void reload();
    }
  }

  useEffect(() => {
    if (items.length === 0 || !listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ" || e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        const step = e.code === "ArrowDown" || e.code === "KeyJ" ? 1 : -1;
        const next = (cursor + step + items.length) % items.length;
        lastCursorIdx.current = next;
        setCursorKey(items[next].key);
        return;
      }
      if ((e.code === "Enter" || e.code === "Space") && target?.tagName === "BUTTON") return;
      const action = branchTreeKeyAction(items[cursor], e.code);
      if (!action) return;
      e.preventDefault();
      switch (action.type) {
        case "toggle-branch": toggleBranch(action.branch); break;
        case "toggle-commit": {
          const item = items[cursor];
          if (item?.kind === "commit") toggleCommit(item.branch, action.hash);
          break;
        }
        case "open-file": openCommitFileZen(action.hash, action.path); break;
        case "open-session": if (!busyBranch) void openSession(action.row); break;
        case "delete": if (!busyBranch) void deleteBranch(action.row); break;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, cursor, listenerActive, busyBranch]);

  if (!loaded && loadError !== null) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
        <span className="text-[11px] text-muted-foreground">Could not load branches</span>
        <span className="max-w-full break-words text-[10px] text-muted-foreground/60">{loadError}</span>
        <button
          type="button"
          disabled={refreshing}
          onClick={() => void reload()}
          className={`${ACTION_BTN} bg-secondary text-foreground hover:bg-secondary/70 disabled:opacity-50`}
        >
          Retry
        </button>
      </div>
    );
  }

  if (!loaded) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6">
        <ProgressBar className="max-w-32" />
        <span className="text-[10px] text-muted-foreground">Loading branches...</span>
      </div>
    );
  }

  const withoutSession = orphanBranchCount(rows);

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/40 px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Branches ({rows.length}){withoutSession > 0 && ` · ${withoutSession} without a session`}
        </span>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                aria-label="Refresh branches"
                disabled={refreshing}
                onClick={() => void reload()}
                className="flex size-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary disabled:opacity-50"
              />
            }
          >
            <RefreshCw size={10} className={refreshing ? "animate-spin" : ""} />
          </TooltipTrigger>
          <TooltipContent side="bottom" className="text-[10px]">Refresh</TooltipContent>
        </Tooltip>
      </div>
      {refreshing && <ProgressBar />}
      {rows.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-6 py-10">
          <div className="text-center">
            <GitBranch size={20} className="mx-auto mb-2 text-muted-foreground/40" />
            <p className="text-[11px] text-muted-foreground/80">No unpushed branches</p>
            <p className="mt-1 text-[10px] text-muted-foreground/50">Local nergal/* branches with commits on no remote show up here.</p>
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto">
          {rows.map((row) => {
            const rowKey = branchKey(row.name);
            const isCursor = cursorItemKey === rowKey;
            const isExpanded = expandedBranch === row.name;
            const commits = branchCommits[row.name];
            const allowed = branchActionsAllowed(row);
            return (
              <div key={row.name}>
                <div
                  ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                  onMouseEnter={() => setCursorKey(rowKey)}
                  onClick={() => toggleBranch(row.name)}
                  className={`group flex cursor-pointer flex-col gap-0.5 border-l-2 px-3 py-1.5 transition-colors ${
                    isCursor
                      ? "border-l-orange-500 bg-orange-500/10"
                      : isExpanded
                        ? "border-l-transparent bg-secondary/40"
                        : "border-l-transparent hover:bg-secondary/30"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground/90">{row.name}</span>
                    <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                      {row.pending ?? "?"} unpushed
                    </span>
                  </div>
                  <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                    <span className="shrink-0">{relativeDate(row.last_commit_unix)}</span>
                    <span className="min-w-0 flex-1 truncate">{row.last_subject}</span>
                  </div>
                  <div className="flex items-center gap-1.5">
                    {row.session ? (
                      <span className="truncate rounded bg-primary/15 px-1.5 text-[9px] text-primary">{row.session.name}</span>
                    ) : (
                      <span className="rounded bg-secondary px-1.5 text-[9px] text-muted-foreground">no session</span>
                    )}
                    {row.in_use && (
                      <span className="rounded bg-yellow-500/15 px-1.5 text-[9px] text-yellow-400">in use</span>
                    )}
                    {row.missing_worktree && (
                      <Tooltip>
                        <TooltipTrigger
                          render={<span tabIndex={0} className="rounded bg-red-500/15 px-1.5 text-[9px] text-red-400 focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary" />}
                        >
                          worktree missing
                        </TooltipTrigger>
                        <TooltipContent side="bottom" className="font-mono text-[10px]">{row.missing_worktree}</TooltipContent>
                      </Tooltip>
                    )}
                    {allowed && (
                      <div
                        className={`ml-auto flex items-center gap-1 group-hover:opacity-100 focus-within:opacity-100 ${
                          isCursor ? "opacity-100" : "opacity-0"
                        }`}
                      >
                        <button
                          type="button"
                          disabled={busyBranch !== null}
                          aria-label={`Open session on ${row.name}`}
                          onClick={(e) => { e.stopPropagation(); void openSession(row); }}
                          className={`${ACTION_BTN} bg-primary/15 text-primary hover:bg-primary/25 disabled:opacity-50`}
                        >
                          Open session
                        </button>
                        <button
                          type="button"
                          disabled={busyBranch !== null}
                          aria-label={`Delete branch ${row.name}`}
                          onClick={(e) => { e.stopPropagation(); void deleteBranch(row); }}
                          className={`${ACTION_BTN} bg-red-500/10 text-red-400 hover:bg-red-500/20 disabled:opacity-50`}
                        >
                          Delete
                        </button>
                      </div>
                    )}
                  </div>
                </div>
                {isExpanded && !commits && (
                  <div className="ml-6 mb-1 flex flex-col gap-1 px-2 py-0.5">
                    <span className="text-[10px] text-muted-foreground/50">Loading commits...</span>
                    <ProgressBar className="max-w-24" />
                  </div>
                )}
                {isExpanded && commits && commits.length === 0 && (
                  <div className="ml-6 mb-1 px-2 py-0.5 text-[10px] text-muted-foreground/50">No unpushed commits</div>
                )}
                {isExpanded && commits && commits.map((c) => {
                  const commitOpen = expandedCommit === c.hash;
                  const files = commitFiles[c.hash];
                  const commitKey = `c:${row.name}:${c.hash}`;
                  return (
                    <div key={c.hash} className="ml-4">
                      <button
                        type="button"
                        ref={(el) => { if (el && cursorItemKey === commitKey) el.scrollIntoView({ block: "nearest" }); }}
                        onMouseEnter={() => setCursorKey(commitKey)}
                        onClick={() => toggleCommit(row.name, c.hash)}
                        className={`flex w-full items-center gap-2 rounded px-2 py-0.5 text-left transition-colors focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary ${
                          cursorItemKey === commitKey ? "bg-orange-500/10" : "hover:bg-secondary/30"
                        }`}
                      >
                        <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">{c.hash}</span>
                        <span className="truncate text-[11px] text-foreground/80">{c.message}</span>
                      </button>
                      {commitOpen && files && files.length > 0 && (
                        <div className="ml-6 mb-1">
                          {files.map((f) => {
                            const fileKey = `f:${row.name}:${c.hash}:${f}`;
                            return (
                            <button
                              key={f}
                              type="button"
                              ref={(el) => { if (el && cursorItemKey === fileKey) el.scrollIntoView({ block: "nearest" }); }}
                              onMouseEnter={() => setCursorKey(fileKey)}
                              onClick={() => openCommitFileZen(c.hash, f)}
                              className={`flex w-full items-center gap-1.5 rounded px-2 py-0.5 text-left text-[10px] transition-colors hover:text-foreground focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary ${
                                cursorItemKey === fileKey ? "bg-orange-500/10 text-foreground" : "text-muted-foreground hover:bg-secondary/30"
                              }`}
                            >
                              <span className="size-1 shrink-0 rounded-full bg-muted-foreground/40" />
                              <span className="truncate">{f.split("/").pop() ?? f}</span>
                            </button>
                            );
                          })}
                        </div>
                      )}
                      {commitOpen && files && files.length === 0 && (
                        <div className="ml-6 mb-1 px-2 py-0.5 text-[10px] text-muted-foreground/50">No files</div>
                      )}
                      {commitOpen && !files && (
                        <div className="ml-6 mb-1 flex flex-col gap-1 px-2 py-0.5">
                          <span className="text-[10px] text-muted-foreground/50">Loading files...</span>
                          <ProgressBar className="max-w-24" />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            );
          })}
          <div className="sticky bottom-0 flex items-center gap-1 border-t border-border/40 bg-card/95 px-3 py-1 text-[9px] text-muted-foreground/60">
            <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> · <Kbd keys="j" /><Kbd keys="k" /> move · <Kbd keys="space" /> expand · <Kbd keys="enter" /> open file · <Kbd keys="o" /> open session · <Kbd keys="d" /> delete
          </div>
        </div>
      )}
    </div>
  );
}
