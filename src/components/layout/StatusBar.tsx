import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionAtom, activeSessionIdAtom, activeCostAtom, activeModeAtom } from "@/stores/workspace";
import { activeGitInfoAtom, refreshGitInfoAtom } from "@/stores/git";
import { loadSessionFilesAtom } from "@/stores/files";
import { activitySummaryAtom, activityDrawerOpenAtom } from "@/stores/activity";
import { Badge } from "@/components/ui/badge";
import { GitBranch, Zap, ChevronUp } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}m ${secs}s`;
}

const modeColors: Record<string, string> = {
  idle: "bg-muted-foreground",
  thinking: "bg-yellow-500",
  tool: "bg-blue-500",
  responding: "bg-green-500",
};

interface StatusBarProps {
  layoutPreset?: string;
}

export function StatusBar({ layoutPreset }: StatusBarProps) {
  const session = useAtomValue(activeSessionAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const mode = useAtomValue(activeModeAtom);
  const cost = useAtomValue(activeCostAtom);
  const gitInfo = useAtomValue(activeGitInfoAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);
  const loadFiles = useSetAtom(loadSessionFilesAtom);
  const summary = useAtomValue(activitySummaryAtom);
  const setDrawerOpen = useSetAtom(activityDrawerOpenAtom);

  useEffect(() => {
    if (sessionId) {
      refreshGit(sessionId);
      loadFiles(sessionId);
    }
  }, [sessionId]);

  const dotColor = modeColors[mode] ?? "bg-muted-foreground";

  return (
    <footer
      className="flex h-7 items-center justify-between border-t border-border bg-background px-3 text-xs"
      role="status"
    >
      {/* Left: git info + mode */}
      <div className="flex items-center gap-2">
        {gitInfo && (
          <div className="flex items-center gap-1 text-muted-foreground">
            <GitBranch className="size-3" />
            <Tooltip>
              <TooltipTrigger className="cursor-default max-w-40 truncate">
                {gitInfo.branch}
              </TooltipTrigger>
              <TooltipContent>{gitInfo.branch}</TooltipContent>
            </Tooltip>
            {gitInfo.dirty && (
              <span className="inline-block size-1.5 rounded-full bg-orange-500" aria-label="Uncommitted changes" />
            )}
            {gitInfo.ahead > 0 && (
              <span className="text-[10px] text-muted-foreground/70">+{gitInfo.ahead}</span>
            )}
          </div>
        )}
        <Badge
          variant="secondary"
          className="h-4 gap-1 px-1.5 text-[10px]"
        >
          <span
            className={`inline-block size-1.5 rounded-full ${dotColor} ${mode !== "idle" ? "animate-dot-pulse" : ""}`}
            aria-hidden="true"
          />
          {mode}
        </Badge>
      </div>

      {/* Center: activity summary (clickable to open drawer) + layout preset */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => setDrawerOpen((prev) => !prev)}
          className="flex items-center gap-1.5 rounded px-2 py-0.5 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          {summary.lastAction ? (
            <>
              <Zap className="size-3 text-primary" />
              <span className="max-w-48 truncate">{summary.lastAction}</span>
              <span className="text-muted-foreground/60">│</span>
              <span>{summary.actionCount} actions</span>
              <span className="text-muted-foreground/60">│</span>
              <span>{formatElapsed(summary.elapsedSeconds)}</span>
            </>
          ) : (
            <span>No activity</span>
          )}
          <ChevronUp className="ml-1 size-3" />
        </button>

        {layoutPreset && (
          <span className="rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {layoutPreset}
          </span>
        )}
      </div>

      {/* Right: session ID + tokens + cost */}
      <div className="flex items-center gap-2 text-muted-foreground">
        {session && (
          <Tooltip>
            <TooltipTrigger className="cursor-default">
              {session.id.slice(0, 8)}
            </TooltipTrigger>
            <TooltipContent>{session.id}</TooltipContent>
          </Tooltip>
        )}
        <Tooltip>
          <TooltipTrigger className="cursor-default">
            <span className="flex items-center gap-1.5">
              <span>in:{formatTokens(cost.input_tokens ?? 0)}</span>
              <span>out:{formatTokens(cost.output_tokens ?? 0)}</span>
            </span>
          </TooltipTrigger>
          <TooltipContent>
            <div className="flex flex-col gap-0.5 text-left">
              <span>Input: {cost.input_tokens.toLocaleString()}</span>
              <span>Output: {cost.output_tokens.toLocaleString()}</span>
              {cost.cache_read > 0 && (
                <span>Cache read: {cost.cache_read.toLocaleString()}</span>
              )}
            </div>
          </TooltipContent>
        </Tooltip>
        <span className="text-foreground font-medium">
          ${(cost.total_usd ?? 0).toFixed(4)}
        </span>
      </div>
    </footer>
  );
}
