import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionIdAtom, activeModeAtom, activeCwdAtom, activeStatusLineAtom } from "@/stores/workspace";
import { activeGitInfoAtom, refreshGitInfoAtom } from "@/stores/git";
import { loadSessionFilesAtom } from "@/stores/files";
import { activitySummaryAtom, activityDrawerOpenAtom } from "@/stores/activity";
import { activeAgentMetadataAtom } from "@/stores/agent";
import {
  browserNewTabAction,
  browserSetModeAction,
  localhostPortsAtom,
} from "@/stores/browser";
import { openTabAction } from "@/stores/rightPanel";
import { Badge } from "@/components/ui/badge";
import { GitBranch, FolderOpen, Zap, ChevronUp, Gauge, Clock, Globe } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (mins >= 60) {
    const hrs = Math.floor(mins / 60);
    return `${hrs}h ${mins % 60}m`;
  }
  return `${mins}m ${secs}s`;
}

function formatDuration(ms: number | null): string {
  if (ms == null) return "--";
  return formatElapsed(Math.floor(ms / 1000));
}

function rateLimitColor(pct: number | null): string {
  if (pct == null) return "text-muted-foreground";
  if (pct >= 90) return "text-red-500";
  if (pct >= 70) return "text-yellow-500";
  return "text-muted-foreground";
}

function contextBarColor(pct: number | null): string {
  if (pct == null) return "bg-muted-foreground/30";
  if (pct >= 90) return "bg-red-500";
  if (pct >= 70) return "bg-yellow-500";
  return "bg-primary";
}

function modeDotColor(mode: string): string {
  if (mode === "idle") return "bg-muted-foreground";
  if (mode === "active") return "bg-sky-400";
  return "bg-green-500";
}

export function StatusBar() {
  const sessionId = useAtomValue(activeSessionIdAtom);
  const mode = useAtomValue(activeModeAtom);
  const gitInfo = useAtomValue(activeGitInfoAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const loadFiles = useSetAtom(loadSessionFilesAtom);
  const cwd = useAtomValue(activeCwdAtom);
  const summary = useAtomValue(activitySummaryAtom);
  const setDrawerOpen = useSetAtom(activityDrawerOpenAtom);
  const sl = useAtomValue(activeStatusLineAtom);
  const agentMeta = useAtomValue(activeAgentMetadataAtom);

  useEffect(() => {
    if (sessionId) {
      refreshGit(sessionId);
      loadFiles(sessionId);
    }
  }, [sessionId]);

  const dotColor = modeDotColor(mode);
  const ctxPct = sl.context_used_pct != null ? Math.round(sl.context_used_pct) : null;

  return (
    <footer
      className="flex h-7 items-center justify-between bg-card px-3 text-[11px] leading-none"
      role="status"
    >
      {/* Left: git info + cwd + mode */}
      <div className="flex items-center gap-2 text-muted-foreground">
        {agentMeta && (
          <Tooltip>
            <TooltipTrigger className="cursor-default">
              <Badge variant="outline" className="h-4 px-1.5 text-[9px] font-medium uppercase tracking-wider">
                {agentMeta.display_name}
              </Badge>
            </TooltipTrigger>
            <TooltipContent>Active agent: {agentMeta.id}</TooltipContent>
          </Tooltip>
        )}
        {gitInfo && (
          <div className="flex items-center gap-1">
            <GitBranch className="size-3 shrink-0" />
            <Tooltip>
              <TooltipTrigger className="cursor-default max-w-40 truncate">
                {gitInfo.branch}
              </TooltipTrigger>
              <TooltipContent>{gitInfo.branch}</TooltipContent>
            </Tooltip>
            {gitInfo.dirty && (
              <span className="inline-block size-1.5 shrink-0 rounded-full bg-orange-500" aria-label="Uncommitted changes" />
            )}
            {gitInfo.ahead > 0 && (
              <span className="text-muted-foreground/70">+{gitInfo.ahead}</span>
            )}
          </div>
        )}
        {cwd && (
          <div className="flex items-center gap-1">
            <FolderOpen className="size-3 shrink-0" />
            <Tooltip>
              <TooltipTrigger className="cursor-default max-w-32 truncate">
                {cwd.split("/").pop() ?? cwd}
              </TooltipTrigger>
              <TooltipContent>{cwd}</TooltipContent>
            </Tooltip>
          </div>
        )}
        <Badge
          variant="secondary"
          className="h-4 gap-1 px-1.5 text-[11px] leading-none"
        >
          <span
            className={`inline-block size-1.5 shrink-0 rounded-full ${dotColor}`}
            aria-hidden="true"
          />
          {mode}
        </Badge>
      </div>

      {/* Center: activity summary + localhost ports */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => setDrawerOpen((prev) => !prev)}
          className="flex items-center gap-1.5 rounded px-2 py-0.5 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          {summary.lastAction ? (
            <>
              <Zap className="size-3 shrink-0 text-primary" />
              <span className="max-w-48 truncate">{summary.lastAction}</span>
              <span className="text-muted-foreground/60">│</span>
              <span>{summary.actionCount} actions</span>
              <span className="text-muted-foreground/60">│</span>
              <span>{formatElapsed(summary.elapsedSeconds)}</span>
            </>
          ) : (
            <span>No activity</span>
          )}
          <ChevronUp className="ml-1 size-3 shrink-0" />
        </button>

        <LocalhostPortChips />
      </div>

      {/* Right: context %, rate limits, model, duration */}
      <div className="flex items-center gap-2.5 text-muted-foreground">
        {ctxPct != null && (
          <Tooltip>
            <TooltipTrigger className="flex cursor-default items-center gap-1">
              <Gauge className="size-3 shrink-0" />
              <div className="flex h-2 w-12 items-center overflow-hidden rounded-full bg-muted">
                <div
                  className={`h-full rounded-full transition-all ${contextBarColor(ctxPct)}`}
                  style={{ width: `${Math.min(ctxPct, 100)}%` }}
                />
              </div>
              <span>{ctxPct}%</span>
            </TooltipTrigger>
            <TooltipContent>
              Context window: {ctxPct}% used
              {sl.context_window_size && ` (${(sl.context_window_size / 1000).toFixed(0)}K)`}
            </TooltipContent>
          </Tooltip>
        )}

        {sl.rate_5h_pct != null && (
          <Tooltip>
            <TooltipTrigger className={`cursor-default ${rateLimitColor(sl.rate_5h_pct)}`}>
              5h:{Math.round(sl.rate_5h_pct)}%
              {sl.rate_5h_resets_at && (
                <span className="text-muted-foreground/60"> {new Date(sl.rate_5h_resets_at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
              )}
            </TooltipTrigger>
            <TooltipContent>
              5-hour rate limit: {sl.rate_5h_pct.toFixed(1)}% used
              {sl.rate_5h_resets_at && (
                <> — resets {new Date(sl.rate_5h_resets_at * 1000).toLocaleTimeString()}</>
              )}
            </TooltipContent>
          </Tooltip>
        )}

        {sl.rate_7d_pct != null && (
          <Tooltip>
            <TooltipTrigger className={`cursor-default ${rateLimitColor(sl.rate_7d_pct)}`}>
              7d:{Math.round(sl.rate_7d_pct)}%
              {sl.rate_7d_resets_at && (
                <span className="text-muted-foreground/60"> {new Date(sl.rate_7d_resets_at * 1000).toLocaleDateString([], { weekday: "short" })}</span>
              )}
            </TooltipTrigger>
            <TooltipContent>
              7-day rate limit: {sl.rate_7d_pct.toFixed(1)}% used
              {sl.rate_7d_resets_at && (
                <> — resets {new Date(sl.rate_7d_resets_at * 1000).toLocaleDateString()}</>
              )}
            </TooltipContent>
          </Tooltip>
        )}

        {sl.model_name && (
          <Tooltip>
            <TooltipTrigger className="cursor-default font-medium text-foreground">
              {sl.model_name}
            </TooltipTrigger>
            <TooltipContent>{sl.model_id ?? sl.model_name}</TooltipContent>
          </Tooltip>
        )}

        {sl.duration_ms != null && (
          <span className="flex items-center gap-0.5">
            <Clock className="size-3 shrink-0" />
            {formatDuration(sl.duration_ms)}
          </span>
        )}
      </div>
    </footer>
  );
}

/// Renders one chip per localhost port detected by the Rust scanner. Click
/// opens (or focuses) the browser panel and navigates to that port.
function LocalhostPortChips() {
  const ports = useAtomValue(localhostPortsAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const newTab = useSetAtom(browserNewTabAction);
  const setMode = useSetAtom(browserSetModeAction);
  const openTab = useSetAtom(openTabAction);

  if (ports.length === 0) return null;

  async function openPort(port: number) {
    if (!sessionId) return;
    setMode({ sessionId, mode: "dock" });
    openTab({
      tab: { id: `browser:${sessionId}`, type: "browser", label: "Browser" },
    });
    try {
      await newTab({ sessionId, url: `http://localhost:${port}` });
    } catch {
      /* validate_url fails on a sane localhost URL only if the user fed us
         garbage — ignore here. */
    }
  }

  return (
    <div className="flex items-center gap-1">
      <Globe className="size-3 shrink-0 text-muted-foreground/60" />
      {ports.map((port) => (
        <button
          key={port}
          type="button"
          onClick={() => openPort(port)}
          disabled={!sessionId}
          title={`Open http://localhost:${port}`}
          className="rounded border border-border/40 px-1.5 py-0 font-mono text-[10px] text-muted-foreground transition-colors hover:border-primary/40 hover:bg-secondary hover:text-foreground disabled:opacity-40"
        >
          :{port}
        </button>
      ))}
    </div>
  );
}
