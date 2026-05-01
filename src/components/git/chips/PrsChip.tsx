import { useEffect, useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import type { PrSummary } from "@/stores/git";
import { openPrTabAction } from "@/stores/git";

interface PrsChipProps {
  sessionId: string;
  workspaceId: string | null;
}

/// Phase 6 stub: keeps the existing list-only UI to avoid losing the PRs view
/// during the chip migration. PR Viewer integration lands in phase 6 when we
/// migrate `PrViewer.tsx` into the chip's bottom area.
export function PrsChip({ sessionId: _sessionId, workspaceId }: PrsChipProps) {
  const [prs, setPrs] = useState<PrSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const openPrTab = useSetAtom(openPrTabAction);
  const _ws = useAtomValue;
  void _ws;

  useEffect(() => {
    if (!workspaceId) { setLoading(false); return; }
    invoke<PrSummary[]>("list_prs", { workspaceId })
      .then((rows) => { setPrs(rows); setLoading(false); })
      .catch(() => setLoading(false));
  }, [workspaceId]);

  if (!workspaceId) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[10px] text-muted-foreground">No workspace bound to this session</span>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[10px] text-muted-foreground">Loading PRs...</span>
      </div>
    );
  }

  if (prs.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="text-center">
          <p className="text-[11px] text-muted-foreground/80">No PRs yet</p>
          <p className="mt-1 text-[10px] text-muted-foreground/50">Ship a session to create one</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/40 px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          PRs ({prs.length})
        </span>
        <span className="text-[9px] text-muted-foreground/50">embedded viewer in phase 6</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {prs.map((pr) => (
          <button
            key={pr.number}
            onClick={() => openPrTab({ workspaceId, pr })}
            className="flex w-full items-center gap-2 px-3 py-1 text-left transition-colors hover:bg-secondary/30"
          >
            <span className={`shrink-0 rounded px-1.5 py-0.5 font-mono text-[9px] ${
              pr.state === "OPEN"
                ? "bg-green-500/15 text-green-400"
                : pr.state === "MERGED"
                ? "bg-purple-500/15 text-purple-400"
                : "bg-muted text-muted-foreground"
            }`}>
              #{pr.number}
            </span>
            <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">{pr.title}</span>
            <span className="shrink-0 font-mono text-[9px] text-muted-foreground/60">
              {pr.head_ref_name} → {pr.base_ref_name}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
