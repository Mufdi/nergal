import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeActivityAtom, activityDrawerOpenAtom } from "@/stores/activity";
import type { ActivityEntry } from "@/lib/types";
import { PulseDots } from "@/components/ui/PulseDots";
import { X, Zap, ChevronDown, ChevronRight, Search } from "lucide-react";

const TYPE_COLORS: Record<ActivityEntry["type"], string> = {
  tool_use: "bg-blue-500",
  session: "bg-orange-500",
  task: "bg-green-500",
  plan: "bg-primary",
  error: "bg-destructive",
  file_modified: "bg-yellow-500",
};

/// Filter chips, important types first.
const TYPE_ORDER: ActivityEntry["type"][] = [
  "tool_use",
  "file_modified",
  "task",
  "error",
  "plan",
  "session",
];

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60000)}m${Math.round((ms % 60000) / 1000)}s`;
}

function basename(p: string): string {
  const seg = p.split("/").filter(Boolean);
  return seg[seg.length - 1] ?? p;
}

export function ActivityDrawer() {
  const entries = useAtomValue(activeActivityAtom);
  const isOpen = useAtomValue(activityDrawerOpenAtom);
  const setOpen = useSetAtom(activityDrawerOpenAtom);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [activeTypes, setActiveTypes] = useState<Set<ActivityEntry["type"]>>(new Set());

  if (!isOpen) return null;

  const reversed = [...entries].reverse();
  const q = query.trim().toLowerCase();
  const filtered = reversed.filter((e) => {
    if (activeTypes.size > 0 && !activeTypes.has(e.type)) return false;
    if (q) {
      const hay = `${e.message} ${e.detail ?? ""} ${(e.files ?? []).join(" ")}`.toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });
  const isFiltering = q.length > 0 || activeTypes.size > 0;

  function toggleThinking(id: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleType(t: ActivityEntry["type"]) {
    setActiveTypes((prev) => {
      const next = new Set(prev);
      if (next.has(t)) next.delete(t);
      else next.add(t);
      return next;
    });
  }

  return (
    <div className="flex flex-col rounded-lg border-2 border-border bg-card" style={{ maxHeight: "30vh" }}>
      {/* Header */}
      <div className="flex h-8 items-center justify-between border-b border-border px-3">
        <div className="flex items-center gap-2">
          <Zap className="size-3 text-primary" />
          <span className="text-xs font-medium text-foreground">Activity Timeline</span>
          <span className="text-[10px] text-muted-foreground">
            ({isFiltering ? `${filtered.length}/${entries.length}` : entries.length} events)
          </span>
        </div>
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </div>

      {/* Filters: search + per-type toggles */}
      <div className="flex items-center gap-2 border-b border-border/30 px-3 py-1">
        <div className="flex items-center gap-1 rounded bg-background/60 px-1.5">
          <Search className="size-2.5 shrink-0 text-muted-foreground/50" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter…"
            className="h-5 w-24 bg-transparent text-[10px] text-foreground outline-none placeholder:text-muted-foreground/50"
          />
        </div>
        <div className="flex items-center gap-0.5 overflow-x-auto scrollbar-none">
          {TYPE_ORDER.map((t) => {
            const active = activeTypes.has(t);
            return (
              <button
                key={t}
                type="button"
                onClick={() => toggleType(t)}
                className={`flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[9px] transition-colors ${
                  active ? "bg-secondary text-foreground" : "text-muted-foreground/50 hover:text-foreground"
                }`}
              >
                <span className={`size-1.5 rounded-full ${TYPE_COLORS[t]}`} aria-hidden="true" />
                {t.replace("_", " ")}
              </button>
            );
          })}
        </div>
      </div>

      {/* Timeline strip (all entries, overview) */}
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
        {filtered.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-xs text-muted-foreground">
              {entries.length === 0 ? "No activity yet" : "No matching activity"}
            </p>
          </div>
        ) : (
          <div className="space-y-px px-2 py-1">
            {filtered.map((entry) => {
              const hasThinking = !!entry.detail && entry.detail.length > 50;
              const isExpanded = expandedIds.has(entry.id);
              const hasFiles = !!entry.files && entry.files.length > 0;
              return (
                <div key={entry.id} className="rounded hover:bg-secondary/50">
                  <div className="flex items-start gap-2 px-2 py-1.5">
                    <span
                      className={`mt-1.5 size-1.5 flex-shrink-0 rounded-full ${TYPE_COLORS[entry.type]}`}
                      aria-hidden="true"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <p className="text-xs text-foreground">{entry.message}</p>
                        {entry.status === "running" && <PulseDots count={3} className="text-primary" />}
                        {entry.durationMs != null && (
                          <span className="shrink-0 text-[9px] tabular-nums text-muted-foreground/60">
                            {formatDuration(entry.durationMs)}
                          </span>
                        )}
                      </div>
                      {hasFiles && (
                        <p className="truncate font-mono text-[10px] text-muted-foreground/80">
                          {entry.files!.map(basename).join(", ")}
                        </p>
                      )}
                      {entry.detail && !hasThinking && !hasFiles && (
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
