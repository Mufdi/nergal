import { useState, useEffect, useCallback } from "react";
import { invoke } from "@/lib/tauri";
import { useAtomValue, useSetAtom } from "jotai";
import { openZenModeAtom, zenModeAtom, zenActiveZoneAtom, prZenAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import { Kbd } from "@/components/ui/kbd";

interface CommitEntry {
  hash: string;
  message: string;
}

interface HistoryChipProps {
  sessionId: string;
  /// When `true`, this History chip is rendered inside the Zen sidebar and
  /// should listen only when zone === "sidebar". Otherwise the underlying
  /// chip would race with the in-Zen DiffView for j/k.
  inZen?: boolean;
}

export function HistoryChip({ sessionId, inZen = false }: HistoryChipProps) {
  const [commits, setCommits] = useState<CommitEntry[]>([]);
  const [expandedCommit, setExpandedCommit] = useState<string | null>(null);
  const [commitFiles, setCommitFiles] = useState<Record<string, string[]>>({});
  const [cursor, setCursor] = useState(0);
  const [loading, setLoading] = useState(true);
  const openZenMode = useSetAtom(openZenModeAtom);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const zenZone = useAtomValue(zenActiveZoneAtom);
  const anyZenOpen = zenState.open || conflictsZen || prZen !== null;
  const listenerActive = inZen
    ? anyZenOpen && zenZone === "sidebar"
    : !anyZenOpen;

  const refresh = useCallback(() => {
    invoke<CommitEntry[]>("get_recent_commits", { sessionId, count: 20 })
      .then((c) => { setCommits(c); setLoading(false); })
      .catch(() => setLoading(false));
  }, [sessionId]);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 15000);
    return () => clearInterval(id);
  }, [refresh]);

  function toggleExpand(hash: string) {
    if (expandedCommit === hash) {
      setExpandedCommit(null);
      return;
    }
    setExpandedCommit(hash);
    if (!commitFiles[hash]) {
      invoke<string[]>("get_commit_files", { sessionId, hash })
        .then((files) => setCommitFiles((prev) => ({ ...prev, [hash]: files })))
        .catch(() => {});
    }
  }

  function openCommitFileZen(_hash: string, filePath: string) {
    const allFilesAcrossCommits = Object.values(commitFiles).flat();
    openZenMode({ filePath, sessionId, files: allFilesAcrossCommits });
  }

  // ↑/↓ + j/k navigate; Space expands; Enter on expanded commit opens first file
  useEffect(() => {
    if (commits.length === 0) return;
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ") {
        e.preventDefault();
        setCursor((i) => (i + 1) % commits.length);
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        setCursor((i) => (i - 1 + commits.length) % commits.length);
        return;
      }
      if (e.code === "Space") {
        const c = commits[cursor];
        if (!c) return;
        e.preventDefault();
        toggleExpand(c.hash);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commits, cursor, listenerActive]);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[10px] text-muted-foreground">Loading history...</span>
      </div>
    );
  }

  if (commits.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[10px] text-muted-foreground">No commits yet</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/40 px-3 py-1">
        <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          History ({commits.length})
        </span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {commits.map((c, i) => {
          const isExpanded = expandedCommit === c.hash;
          const isCursor = cursor === i;
          const files = commitFiles[c.hash] ?? [];
          return (
            <div key={c.hash}>
              <div
                ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                onMouseEnter={() => setCursor(i)}
                onClick={() => toggleExpand(c.hash)}
                className={`flex items-center gap-2 px-3 py-1 transition-colors cursor-pointer border-l-2 ${
                  isCursor
                    ? "border-l-orange-500 bg-orange-500/10"
                    : isExpanded
                    ? "border-l-transparent bg-secondary/40"
                    : "border-l-transparent hover:bg-secondary/30"
                }`}
              >
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground/60">{c.hash}</span>
                <span className="truncate text-[11px] text-foreground/80">{c.message}</span>
              </div>
              {isExpanded && files.length > 0 && (
                <div className="ml-6 mb-1">
                  {files.map((f) => {
                    const name = f.split("/").pop() ?? f;
                    return (
                      <button
                        key={f}
                        type="button"
                        onClick={() => openCommitFileZen(c.hash, f)}
                        className="flex w-full items-center gap-1.5 rounded px-2 py-0.5 text-left text-[10px] text-muted-foreground hover:bg-secondary/30 hover:text-foreground transition-colors"
                      >
                        <span className="size-1 shrink-0 rounded-full bg-muted-foreground/40" />
                        <span className="truncate">{name}</span>
                      </button>
                    );
                  })}
                </div>
              )}
              {isExpanded && files.length === 0 && (
                <div className="ml-6 mb-1 px-2 py-0.5">
                  <span className="text-[10px] text-muted-foreground/50">Loading files...</span>
                </div>
              )}
            </div>
          );
        })}
        <div className="sticky bottom-0 flex items-center gap-1 border-t border-border/40 bg-card/95 px-3 py-1 text-[9px] text-muted-foreground/60">
          <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> · <Kbd keys="j" /><Kbd keys="k" /> move · <Kbd keys="space" /> expand
        </div>
      </div>
    </div>
  );
}
