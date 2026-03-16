import { useEffect, useRef } from "react";
import { useAtomValue } from "jotai";
import { activeActivityAtom } from "@/stores/activity";
import type { ActivityEntry } from "@/lib/types";

const TYPE_DOT_COLORS: Record<ActivityEntry["type"], string> = {
  tool_use: "bg-muted-foreground",
  session: "bg-orange-500",
  task: "bg-green-500",
  plan: "bg-blue-500",
  error: "bg-destructive",
  file_modified: "bg-yellow-500",
};

function formatRelativeTime(ts: number): string {
  const delta = Math.floor((Date.now() - ts) / 1000);
  if (delta < 5) return "now";
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  return `${Math.floor(delta / 3600)}h ago`;
}

export function ActivityLog() {
  const entries = useAtomValue(activeActivityAtom);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries.length]);

  return (
    <section className="flex h-full w-full flex-col" aria-label="Activity log">
      <header className="flex h-9 shrink-0 items-center border-b border-border px-3">
        <h2 className="text-xs font-medium text-muted-foreground">Activity</h2>
      </header>

      {entries.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-xs text-muted-foreground">No activity</p>
        </div>
      ) : (
        <div ref={scrollRef} className="flex-1 overflow-y-auto">
          <div className="space-y-0.5 px-2 py-1">
            {entries.map((entry) => (
              <div
                key={entry.id}
                className="flex items-start gap-2 rounded px-1.5 py-1"
              >
                <span
                  className={`mt-1.5 size-1.5 flex-shrink-0 rounded-full ${TYPE_DOT_COLORS[entry.type]}`}
                  aria-hidden="true"
                />

                <div className="min-w-0 flex-1">
                  <p className="text-xs text-foreground">{entry.message}</p>
                  {entry.detail && (
                    <p className="truncate text-[10px] text-muted-foreground">
                      {entry.detail}
                    </p>
                  )}
                </div>

                <time className="mt-px flex-shrink-0 text-[10px] text-muted-foreground">
                  {formatRelativeTime(entry.timestamp)}
                </time>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
