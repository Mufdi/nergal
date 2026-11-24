import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@/lib/tauri";
import { useAtomValue, useSetAtom } from "jotai";
import { chipKeysActive } from "@/lib/gitChipKeys";
import { cursorIndexFor } from "@/lib/branchTree";
import { historyTreeItems, historyTreeKeyAction } from "@/lib/historyTree";
import { focusZoneAtom } from "@/stores/shortcuts";
import { openZenModeAtom, zenModeAtom, zenActiveZoneAtom, prZenAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import { Hint, HintBar } from "@/components/ui/hint-bar";
import { ProgressBar } from "@/components/ui/ProgressBar";

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
  const [cursorKey, setCursorKey] = useState<string | null>(null);
  const lastCursorIdx = useRef(0);
  const [loading, setLoading] = useState(true);
  const openZenMode = useSetAtom(openZenModeAtom);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const zenZone = useAtomValue(zenActiveZoneAtom);
  const anyZenOpen = zenState.open || conflictsZen || prZen !== null;
  const focusZone = useAtomValue(focusZoneAtom);
  const listenerActive = chipKeysActive({ inZen, anyZenOpen, zenZone, focusZone });
  const items = historyTreeItems(commits, expandedCommit, commitFiles);
  const cursor = cursorIndexFor(items, cursorKey, lastCursorIdx.current);
  lastCursorIdx.current = cursor;
  const cursorItemKey = items[cursor]?.key ?? null;

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
      setCursorKey(`c:${hash}`);
      return;
    }
    setExpandedCommit(hash);
    if (!commitFiles[hash]) {
      invoke<string[]>("get_scoped_commit_files", {
        scope: { kind: "session", sessionId },
        hash,
      })
        .then((files) => setCommitFiles((prev) => ({ ...prev, [hash]: files })))
        .catch(() => {});
    }
  }

  function openCommitFileZen(hash: string, filePath: string) {
    openZenMode({
      filePath,
      sessionId,
      files: commitFiles[hash] ?? [filePath],
      commit: { hash, scope: { kind: "session", sessionId } },
    });
  }

  useEffect(() => {
    if (items.length === 0) return;
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
      if (e.code === "ArrowDown" || e.code === "KeyJ" || e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        const step = e.code === "ArrowDown" || e.code === "KeyJ" ? 1 : -1;
        const next = (cursor + step + items.length) % items.length;
        lastCursorIdx.current = next;
        setCursorKey(items[next].key);
        return;
      }
      if ((e.code === "Enter" || e.code === "Space") && target?.tagName === "BUTTON") return;
      const action = historyTreeKeyAction(items[cursor], e.code);
      if (!action) return;
      e.preventDefault();
      if (action.type === "toggle-commit") toggleExpand(action.hash);
      else openCommitFileZen(action.hash, action.path);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, cursor, listenerActive]);

  if (loading) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-6">
        <ProgressBar className="max-w-32" />
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
      <div className="min-h-0 flex-1 overflow-y-auto">
        {commits.map((c) => {
          const isExpanded = expandedCommit === c.hash;
          const commitKey = `c:${c.hash}`;
          const isCursor = cursorItemKey === commitKey;
          const files = commitFiles[c.hash] ?? [];
          return (
            <div key={c.hash}>
              <div
                ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
                onMouseEnter={() => setCursorKey(commitKey)}
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
                    const fileKey = `f:${c.hash}:${f}`;
                    return (
                      <button
                        key={f}
                        type="button"
                        ref={(el) => { if (el && cursorItemKey === fileKey) el.scrollIntoView({ block: "nearest" }); }}
                        onMouseEnter={() => setCursorKey(fileKey)}
                        onClick={() => openCommitFileZen(c.hash, f)}
                        className={`flex w-full items-center gap-1.5 rounded px-2 py-0.5 text-left text-[10px] transition-colors hover:text-foreground focus:outline-none focus:ring-1 focus:ring-inset focus:ring-primary ${
                          cursorItemKey === fileKey ? "bg-orange-500/10 text-foreground" : "text-muted-foreground hover:bg-secondary/30"
                        }`}
                      >
                        <span className="size-1 shrink-0 rounded-full bg-muted-foreground/40" />
                        <span className="truncate">{name}</span>
                      </button>
                    );
                  })}
                </div>
              )}
              {isExpanded && files.length === 0 && (
                <div className="ml-6 mb-1 flex flex-col gap-1 px-2 py-0.5">
                  <span className="text-[10px] text-muted-foreground/50">Loading files...</span>
                  <ProgressBar className="max-w-24" />
                </div>
              )}
            </div>
          );
        })}
      </div>

      <HintBar>
        <Hint keys={["arrowup", "arrowdown", "j", "k"]}>move</Hint>
        <Hint keys="space">expand</Hint>
        <Hint keys="enter">open file</Hint>
      </HintBar>
    </div>
  );
}
