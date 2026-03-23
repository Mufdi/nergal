import { useAtomValue, useSetAtom } from "jotai";
import { activeActivityAtom, activityDrawerOpenAtom } from "@/stores/activity";
import { activePanelViewAtom, expandRightPanelAtom } from "@/stores/rightPanel";
import type { ActivityEntry } from "@/lib/types";
import { X, ExternalLink, Zap } from "lucide-react";

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

function formatRelativeTime(ts: number): string {
  const delta = Math.floor((Date.now() - ts) / 1000);
  if (delta < 5) return "now";
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  return `${Math.floor(delta / 3600)}h ago`;
}

export function ActivityDrawer() {
  const entries = useAtomValue(activeActivityAtom);
  const isOpen = useAtomValue(activityDrawerOpenAtom);
  const setOpen = useSetAtom(activityDrawerOpenAtom);
  const setPanelView = useSetAtom(activePanelViewAtom);
  const setExpand = useSetAtom(expandRightPanelAtom);

  if (!isOpen) return null;

  const reversed = [...entries].reverse();

  function openDagTab() {
    setOpen(false);
    setPanelView("transcript");
    setExpand((p) => p + 1);
  }

  return (
    <div className="border-t border-border bg-card" style={{ height: "30vh", minHeight: 180 }}>
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
      {entries.length > 0 && (
        <div className="flex h-6 items-center gap-0.5 overflow-x-auto px-3 scrollbar-none">
          {entries.map((entry) => (
            <div
              key={entry.id}
              className={`size-1.5 flex-shrink-0 rounded-full ${TYPE_COLORS[entry.type]} opacity-70`}
              title={`${entry.message} — ${formatRelativeTime(entry.timestamp)}`}
            />
          ))}
        </div>
      )}

      {/* Event list */}
      <div className="flex-1 overflow-y-auto" style={{ height: "calc(30vh - 56px)" }}>
        {reversed.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-xs text-muted-foreground">No activity yet</p>
          </div>
        ) : (
          <div className="space-y-px px-2 py-1">
            {reversed.map((entry) => (
              <div
                key={entry.id}
                className="flex items-start gap-2 rounded px-2 py-1.5 hover:bg-secondary/50"
              >
                <span
                  className={`mt-1.5 size-1.5 flex-shrink-0 rounded-full ${TYPE_COLORS[entry.type]}`}
                  aria-hidden="true"
                />
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-foreground">{entry.message}</p>
                  {entry.detail && (
                    <p className="truncate text-[10px] text-muted-foreground">{entry.detail}</p>
                  )}
                </div>
                <time className="mt-px flex-shrink-0 text-[10px] text-muted-foreground">
                  {formatTime(entry.timestamp)}
                </time>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
