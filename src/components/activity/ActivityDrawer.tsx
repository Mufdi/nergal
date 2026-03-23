import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeActivityAtom, activityDrawerOpenAtom } from "@/stores/activity";
import { openTabAction, expandRightPanelAtom } from "@/stores/rightPanel";
import type { ActivityEntry } from "@/lib/types";
import { X, ExternalLink, Zap, ChevronDown, ChevronRight } from "lucide-react";

const TYPE_COLORS: Record<ActivityEntry["type"], string> = {
  tool_use: "bg-blue-500",
  session: "bg-orange-500",
  task: "bg-green-500",
  plan: "bg-primary",
  error: "bg-destructive",
  file_modified: "bg-yellow-500",
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}


export function ActivityDrawer() {
  const entries = useAtomValue(activeActivityAtom);
  const isOpen = useAtomValue(activityDrawerOpenAtom);
  const setOpen = useSetAtom(activityDrawerOpenAtom);
  const openTab = useSetAtom(openTabAction);
  const setExpand = useSetAtom(expandRightPanelAtom);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  if (!isOpen) return null;

  const reversed = [...entries].reverse();

  function toggleThinking(id: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function openDagTab() {
    setOpen(false);
    openTab({
      tab: { id: "dag-graph", type: "transcript", label: "Activity DAG" },
    });
    setExpand((p) => p + 1);
  }

  return (
    <div className="flex flex-col rounded-lg border-t border-border/50 bg-card" style={{ maxHeight: "30vh" }}>
      {/* Header */}
      <div className="flex h-8 items-center justify-between border-b border-border px-3">
        <div className="flex items-center gap-2">
          <Zap className="size-3 text-primary" />
          <span className="text-xs font-medium text-foreground">Activity Timeline</span>
          <span className="text-[10px] text-muted-foreground">({entries.length} events)</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={openDagTab}
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground"
          >
            <ExternalLink className="size-3" />
            Open as Tab
          </button>
          <button
            type="button"
            onClick={() => setOpen(false)}
            className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      {/* Timeline strip */}
      {entries.length > 1 && (
        <div className="flex h-5 items-center gap-0.5 overflow-x-auto border-b border-border/30 px-3 scrollbar-none">
          {entries.map((entry) => (
            <div
              key={entry.id}
              className={`size-1.5 flex-shrink-0 rounded-full ${TYPE_COLORS[entry.type]} opacity-70`}
              title={entry.message}
            />
          ))}
        </div>
      )}

      {/* Event list */}
      <div className="flex-1 overflow-y-auto">
        {reversed.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-xs text-muted-foreground">No activity yet</p>
          </div>
        ) : (
          <div className="space-y-px px-2 py-1">
            {reversed.map((entry) => {
              const hasThinking = !!entry.detail && entry.detail.length > 50;
              const isExpanded = expandedIds.has(entry.id);
              return (
                <div key={entry.id} className="rounded hover:bg-secondary/50">
                  <div className="flex items-start gap-2 px-2 py-1.5">
                    <span
                      className={`mt-1.5 size-1.5 flex-shrink-0 rounded-full ${TYPE_COLORS[entry.type]}`}
                      aria-hidden="true"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="text-xs text-foreground">{entry.message}</p>
                      {entry.detail && !hasThinking && (
                        <p className="truncate text-[10px] text-muted-foreground">{entry.detail}</p>
                      )}
                      {hasThinking && (
                        <button
                          type="button"
                          onClick={() => toggleThinking(entry.id)}
                          className="mt-0.5 flex items-center gap-0.5 text-[10px] text-muted-foreground hover:text-foreground"
                        >
                          {isExpanded ? <ChevronDown className="size-2.5" /> : <ChevronRight className="size-2.5" />}
                          thinking
                        </button>
                      )}
                    </div>
                    <time className="mt-px flex-shrink-0 text-[10px] text-muted-foreground">
                      {formatTime(entry.timestamp)}
                    </time>
                  </div>
                  {hasThinking && isExpanded && (
                    <div className="mx-2 mb-1.5 ml-6 rounded bg-background/60 px-2 py-1.5">
                      <p className="whitespace-pre-wrap text-[10px] leading-relaxed text-muted-foreground">
                        {entry.detail}
                      </p>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
