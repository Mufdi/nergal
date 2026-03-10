import { useEffect, useRef } from "react";
import { useAtomValue } from "jotai";
import { activityAtom } from "@/stores/activity";
import type { ActivityEntry } from "@/lib/types";

const TYPE_STYLES: Record<ActivityEntry["type"], { icon: string; color: string }> = {
  tool_use: { icon: "\u2699", color: "text-accent" },
  file_modified: { icon: "\u25A0", color: "text-warning" },
  session: { icon: "\u25C6", color: "text-success" },
  task: { icon: "\u25CB", color: "text-text-muted" },
  plan: { icon: "\u25B7", color: "text-accent" },
  error: { icon: "!", color: "text-danger" },
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function ActivityLog() {
  const entries = useAtomValue(activityAtom);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [entries.length]);

  return (
    <section className="flex h-full flex-col" aria-label="Activity log">
      <header className="flex h-8 items-center border-b border-border px-3">
        <h2 className="text-xs font-medium text-text-muted">Activity</h2>
      </header>

      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        {entries.length === 0 ? (
          <p className="px-3 py-4 text-center text-xs text-text-muted">No activity</p>
        ) : (
          entries.map((entry) => {
            const { icon, color } = TYPE_STYLES[entry.type];
            return (
              <div key={entry.id} className="flex items-start gap-2 border-b border-border/50 px-3 py-1.5">
                <span className={`mt-px flex-shrink-0 text-xs ${color}`} aria-hidden="true">
                  {icon}
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-xs text-text">{entry.message}</p>
                  {entry.detail && (
                    <p className="truncate text-[10px] text-text-muted">{entry.detail}</p>
                  )}
                </div>
                <time className="flex-shrink-0 text-[10px] text-text-muted">{formatTime(entry.timestamp)}</time>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}
