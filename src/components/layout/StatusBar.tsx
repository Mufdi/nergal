import { useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeSessionAtom, activeSessionIdAtom, activeCostAtom, activeModeAtom } from "@/stores/workspace";
import { activeGitInfoAtom, refreshGitInfoAtom } from "@/stores/git";
import { Badge } from "@/components/ui/badge";
import { GitBranch } from "lucide-react";
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

const modeColors: Record<string, string> = {
  idle: "bg-muted-foreground",
  thinking: "bg-yellow-500",
  tool: "bg-blue-500",
  responding: "bg-green-500",
};

export function StatusBar() {
  const session = useAtomValue(activeSessionAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const mode = useAtomValue(activeModeAtom);
  const cost = useAtomValue(activeCostAtom);
  const gitInfo = useAtomValue(activeGitInfoAtom);
  const refreshGit = useSetAtom(refreshGitInfoAtom);

  useEffect(() => {
    if (sessionId) refreshGit(sessionId);
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
            className={`inline-block size-1.5 rounded-full ${dotColor}`}
            aria-hidden="true"
          />
          {mode}
        </Badge>
      </div>

      {/* Center: session ID */}
      <div className="flex items-center">
        {session && (
          <Tooltip>
            <TooltipTrigger className="cursor-default text-muted-foreground">
              {session.id.slice(0, 8)}
            </TooltipTrigger>
            <TooltipContent>{session.id}</TooltipContent>
          </Tooltip>
        )}
      </div>

      {/* Right: tokens + cost */}
      <div className="flex items-center gap-2 text-muted-foreground">
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
