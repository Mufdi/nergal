import { useState, useEffect, useCallback, useRef } from "react";
import { useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import { openPrTabAction, type PrSummary } from "@/stores/git";
import { Loader2, GitPullRequest, Check, X, CircleDashed, ExternalLink } from "lucide-react";

interface PrListSidebarProps {
  workspaceId: string;
  /// When true, the sidebar polls every 60s. The parent toggles this off
  /// when the user switches back to the Files view to avoid wasted gh calls.
  active: boolean;
  /// Bump this counter to force a refresh (e.g. after Ship creates a PR).
  refreshSignal: number;
}

export function PrListSidebar({ workspaceId, active, refreshSignal }: PrListSidebarProps) {
  const [prs, setPrs] = useState<PrSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const openPrTab = useSetAtom(openPrTabAction);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchPrs = useCallback(() => {
    invoke<PrSummary[]>("list_prs", { workspaceId })
      .then((result) => {
        setPrs(result);
        setError(null);
      })
      .catch((err: unknown) => {
        setError(String(err));
      })
      .finally(() => setLoading(false));
  }, [workspaceId]);

  useEffect(() => {
    setLoading(true);
    fetchPrs();
  }, [fetchPrs, workspaceId, refreshSignal]);

  useEffect(() => {
    if (!active) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }
    pollRef.current = setInterval(fetchPrs, 60_000);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [active, fetchPrs]);

  function handleOpen(pr: PrSummary) {
    openPrTab({ workspaceId, pr });
  }

  if (loading && prs.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2">
        <Loader2 size={14} className="animate-spin text-muted-foreground" />
        <span className="text-[10px] text-muted-foreground">Loading PRs…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-3 text-center">
        <span className="text-[10px] text-red-400 break-words">{error}</span>
        <button
          onClick={fetchPrs}
          className="rounded bg-secondary px-2 py-0.5 text-[10px] text-foreground hover:bg-secondary/70"
        >
          Retry
        </button>
      </div>
    );
  }

  if (prs.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-3 text-center">
        <GitPullRequest size={16} className="text-muted-foreground/60" />
        <span className="text-[10px] text-muted-foreground">No pull requests in this workspace</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          Pull Requests ({prs.length})
        </span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {prs.map((pr) => (
          <PrRow key={pr.number} pr={pr} onClick={() => handleOpen(pr)} />
        ))}
      </div>
    </div>
  );
}

interface PrRowProps {
  pr: PrSummary;
  onClick: () => void;
}

function PrRow({ pr, onClick }: PrRowProps) {
  const stateColors = pr.state === "OPEN"
    ? "bg-green-500/15 text-green-400"
    : pr.state === "MERGED"
      ? "bg-purple-500/15 text-purple-400"
      : "bg-muted text-muted-foreground";

  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-start gap-2 border-b border-border/30 px-3 py-1.5 text-left transition-colors hover:bg-secondary/30"
      data-nav-item
    >
      <span className={`mt-0.5 shrink-0 rounded px-1 py-0.5 font-mono text-[9px] font-medium ${stateColors}`}>
        #{pr.number}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[11px] text-foreground/90" title={pr.title}>
          {pr.title}
        </p>
        <div className="flex items-center gap-1 text-[9px] text-muted-foreground/70 font-mono">
          <span className="truncate">{pr.head_ref_name}</span>
          <span>→</span>
          <span className="truncate">{pr.base_ref_name}</span>
        </div>
      </div>
      <PrStateIcon state={pr.state} />
    </button>
  );
}

function PrStateIcon({ state }: { state: string }) {
  const cls = "mt-0.5 shrink-0 text-muted-foreground/60";
  if (state === "OPEN") return <CircleDashed size={11} className={cls} />;
  if (state === "MERGED") return <Check size={11} className={cls} />;
  if (state === "CLOSED") return <X size={11} className={cls} />;
  return <ExternalLink size={11} className={cls} />;
}
