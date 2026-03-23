import { useState, useEffect, useCallback, useRef } from "react";
import { useAtomValue } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { activeSessionFilesAtom } from "@/stores/files";
import { ChevronUp, ChevronDown, ChevronRight, ChevronLeft } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

type LineType = "add" | "remove" | "context" | "header" | "collapsed";

interface DiffLine {
  type: LineType;
  content: string;
  oldNum: number | null;
  newNum: number | null;
  hunkIndex: number;
}

interface Hunk {
  startLine: number;
  label: string;
}

function parseDiff(text: string): { lines: DiffLine[]; hunks: Hunk[] } {
  const lines: DiffLine[] = [];
  const hunks: Hunk[] = [];
  let oldNum = 0;
  let newNum = 0;
  let hunkIndex = -1;
  let lastContextEnd = -1;

  for (const raw of text.split("\n")) {
    if (
      raw.startsWith("diff ") ||
      raw.startsWith("index ") ||
      raw.startsWith("--- ") ||
      raw.startsWith("+++ ") ||
      raw.startsWith("new file") ||
      raw.startsWith("deleted file") ||
      raw.startsWith("old mode") ||
      raw.startsWith("new mode")
    ) {
      continue;
    }

    if (raw.startsWith("@@")) {
      hunkIndex++;
      const match = raw.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/);
      if (match) {
        oldNum = parseInt(match[1], 10);
        newNum = parseInt(match[2], 10);
      }

      if (hunkIndex > 0 && lastContextEnd >= 0) {
        const gap = oldNum - lastContextEnd - 1;
        if (gap > 0) {
          lines.push({
            type: "collapsed",
            content: `${gap} unchanged lines`,
            oldNum: null,
            newNum: null,
            // Belongs to the PREVIOUS hunk as its bottom divider
            hunkIndex: hunkIndex - 1,
          });
        }
      }

      const label = match?.[3]?.trim() ?? "";
      hunks.push({ startLine: lines.length, label });
      lines.push({
        type: "header",
        content: label ? `@@ ${label}` : `@@ Line ${oldNum}`,
        oldNum: null,
        newNum: null,
        hunkIndex,
      });
    } else if (raw.startsWith("+")) {
      lines.push({ type: "add", content: raw.slice(1), oldNum: null, newNum, hunkIndex });
      newNum++;
    } else if (raw.startsWith("-")) {
      lines.push({ type: "remove", content: raw.slice(1), oldNum, newNum: null, hunkIndex });
      lastContextEnd = oldNum;
      oldNum++;
    } else {
      const content = raw.startsWith(" ") ? raw.slice(1) : raw;
      if (oldNum > 0 || newNum > 0) {
        lines.push({ type: "context", content, oldNum, newNum, hunkIndex });
        lastContextEnd = oldNum;
        oldNum++;
        newNum++;
      }
    }
  }

  return { lines, hunks };
}

const LINE_BG: Record<LineType, string> = {
  add: "bg-green-500/10",
  remove: "bg-red-500/10",
  context: "",
  header: "",
  collapsed: "",
};

const LINE_TEXT: Record<LineType, string> = {
  add: "text-green-400",
  remove: "text-red-400",
  context: "text-foreground/70",
  header: "text-muted-foreground",
  collapsed: "text-muted-foreground/60",
};

function relativePath(filePath: string): string {
  const parts = filePath.split("/");
  const wtIdx = parts.indexOf(".worktrees");
  if (wtIdx >= 0 && wtIdx + 3 < parts.length) {
    return parts.slice(wtIdx + 3).join("/");
  }
  const projIdx = parts.findIndex((_p, i) => i > 2 && parts[i - 1] === "Projects");
  if (projIdx >= 0 && projIdx + 1 < parts.length) {
    return parts.slice(projIdx + 1).join("/");
  }
  return parts.slice(-3).join("/");
}

interface SideBySideRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

function buildSideBySideRows(lines: DiffLine[]): (DiffLine | SideBySideRow)[] {
  const result: (DiffLine | SideBySideRow)[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (line.type === "header" || line.type === "collapsed") {
      result.push(line);
      i++;
      continue;
    }

    if (line.type === "context") {
      result.push({ left: line, right: line });
      i++;
      continue;
    }

    // Collect adjacent remove+add groups and pair them
    const removes: DiffLine[] = [];
    const adds: DiffLine[] = [];

    while (i < lines.length && lines[i].type === "remove") {
      removes.push(lines[i]);
      i++;
    }
    while (i < lines.length && lines[i].type === "add") {
      adds.push(lines[i]);
      i++;
    }

    const maxLen = Math.max(removes.length, adds.length);
    for (let j = 0; j < maxLen; j++) {
      result.push({
        left: j < removes.length ? removes[j] : null,
        right: j < adds.length ? adds[j] : null,
      });
    }
  }

  return result;
}

interface SharedContentProps {
  lines: DiffLine[];
  activeHunk: number;
  collapsedHunks: Set<number>;
  hunkRefs: React.RefObject<Map<number, HTMLDivElement>>;
  setActiveHunk: (idx: number) => void;
}

function renderHeaderRow(
  line: DiffLine,
  i: number,
  activeHunk: number,
  collapsedHunks: Set<number>,
  hunkRefs: React.RefObject<Map<number, HTMLDivElement>>,
  setActiveHunk: (idx: number) => void,
  lines: DiffLine[],
  colSpan?: number,
) {
  const isActive = line.hunkIndex === activeHunk;
  const isCollapsed = collapsedHunks.has(line.hunkIndex);
  const inner = (
    <div
      key={i}
      ref={(el) => { if (el) hunkRefs.current.set(line.hunkIndex, el); }}
      onClick={() => { setActiveHunk(line.hunkIndex); }}
      style={{ scrollMarginTop: "8px" }}
      className={`flex cursor-pointer items-center gap-1 border-t px-2 py-0.5 transition-colors ${
        isActive
          ? "border-t-blue-500/40 border-l-2 border-l-blue-500 bg-blue-500/15"
          : "border-t-border/30 bg-secondary/30 hover:bg-secondary/50"
      }`}
    >
      {isCollapsed
        ? <ChevronRight size={10} className="shrink-0 text-muted-foreground/60" />
        : <ChevronLeft size={10} className="shrink-0 text-muted-foreground/40" />
      }
      <span className={`text-[10px] truncate ${isActive ? "text-blue-400" : "text-muted-foreground"}`}>
        {line.content}
      </span>
      {isCollapsed && (
        <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/50">
          {lines.filter((l) => l.hunkIndex === line.hunkIndex && l.type !== "header" && l.type !== "collapsed").length} lines
        </span>
      )}
    </div>
  );

  if (colSpan != null) {
    return <tr key={i}><td colSpan={colSpan}>{inner}</td></tr>;
  }
  return inner;
}

function renderCollapsedRow(
  line: DiffLine,
  i: number,
  activeHunk: number,
  colSpan?: number,
) {
  const isBottom = line.hunkIndex === activeHunk;
  const inner = (
    <div
      key={colSpan != null ? undefined : i}
      className={`flex items-center justify-center border-y py-0.5 transition-colors ${
        isBottom
          ? "border-blue-500/40 bg-blue-500/5"
          : "border-border/30 bg-secondary/20"
      }`}
    >
      <span className={`text-[10px] ${isBottom ? "text-blue-400/60" : "text-muted-foreground/50"}`}>
        {line.content}
      </span>
    </div>
  );

  if (colSpan != null) {
    return <tr key={i}><td colSpan={colSpan}>{inner}</td></tr>;
  }
  return inner;
}

function UnifiedContent({ lines, activeHunk, collapsedHunks, hunkRefs, setActiveHunk }: SharedContentProps) {
  return (
    <>
      {lines.map((line, i) => {
        if (line.type === "collapsed") {
          return renderCollapsedRow(line, i, activeHunk);
        }

        if (line.type === "header") {
          return renderHeaderRow(line, i, activeHunk, collapsedHunks, hunkRefs, setActiveHunk, lines);
        }

        if (collapsedHunks.has(line.hunkIndex)) {
          return null;
        }

        const numColor = line.type === "add"
          ? "text-green-400/50"
          : line.type === "remove"
            ? "text-red-400/50"
            : "text-muted-foreground/30";

        return (
          <div key={i} className={`flex ${LINE_BG[line.type]}`}>
            <span className={`inline-block w-10 shrink-0 select-none pr-1 text-right ${numColor}`}>
              {line.oldNum ?? ""}
            </span>
            <span className={`inline-block w-10 shrink-0 select-none pr-1 text-right ${numColor}`}>
              {line.newNum ?? ""}
            </span>
            <span className={`inline-block w-4 shrink-0 select-none text-center ${LINE_TEXT[line.type]}`}>
              {line.type === "add" ? "+" : line.type === "remove" ? "-" : " "}
            </span>
            <span className={`flex-1 whitespace-pre ${LINE_TEXT[line.type]}`}>
              {line.content}
            </span>
          </div>
        );
      })}
    </>
  );
}

function SideBySideCellContent({ line }: { line: DiffLine | null }) {
  if (!line) {
    return <div className="h-[18px] bg-secondary/10" />;
  }

  const type = line.type;
  const numColor = type === "add"
    ? "text-green-400/50"
    : type === "remove"
      ? "text-red-400/50"
      : "text-muted-foreground/30";
  const displayNum = line.oldNum ?? line.newNum;

  return (
    <div className={`flex ${LINE_BG[type]}`}>
      <span className={`inline-block w-10 shrink-0 select-none pr-1 text-right ${numColor}`}>
        {displayNum ?? ""}
      </span>
      <span className={`inline-block w-4 shrink-0 select-none text-center ${LINE_TEXT[type]}`}>
        {type === "add" ? "+" : type === "remove" ? "-" : " "}
      </span>
      <span className={`flex-1 whitespace-pre-wrap break-all ${LINE_TEXT[type]}`}>
        {line.content}
      </span>
    </div>
  );
}

function SideBySideContent({ lines, activeHunk, collapsedHunks, hunkRefs, setActiveHunk }: SharedContentProps) {
  const rows = buildSideBySideRows(lines);

  return (
    <table className="w-full border-collapse table-fixed">
      <colgroup>
        <col className="w-1/2" />
        <col className="w-1/2" />
      </colgroup>
      <tbody>
        {rows.map((row, i) => {
          // Full-width rows for header/collapsed
          if ("type" in row) {
            if (row.type === "collapsed") {
              return renderCollapsedRow(row, i, activeHunk, 2);
            }
            if (row.type === "header") {
              if (collapsedHunks.has(row.hunkIndex)) {
                return renderHeaderRow(row, i, activeHunk, collapsedHunks, hunkRefs, setActiveHunk, lines, 2);
              }
              return renderHeaderRow(row, i, activeHunk, collapsedHunks, hunkRefs, setActiveHunk, lines, 2);
            }
            return null;
          }

          // Skip collapsed hunk content
          const hunkIdx = row.left?.hunkIndex ?? row.right?.hunkIndex ?? -1;
          if (hunkIdx >= 0 && collapsedHunks.has(hunkIdx)) {
            return null;
          }

          // For context lines on the left, show oldNum; on the right, show newNum
          const leftLine = row.left
            ? row.left.type === "context"
              ? { ...row.left, newNum: null }
              : row.left
            : null;
          const rightLine = row.right
            ? row.right.type === "context"
              ? { ...row.right, oldNum: null }
              : row.right
            : null;

          return (
            <tr key={i}>
              <td className="border-r border-border/20 p-0 px-1 align-top">
                <SideBySideCellContent line={leftLine} />
              </td>
              <td className="p-0 px-1 align-top">
                <SideBySideCellContent line={rightLine} />
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

interface DiffViewProps {
  filePath: string;
  sessionId: string;
  sideBySide?: boolean;
}

export function DiffView({ filePath, sessionId, sideBySide = false }: DiffViewProps) {
  const [lines, setLines] = useState<DiffLine[]>([]);
  const [hunks, setHunks] = useState<Hunk[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeHunk, setActiveHunk] = useState(0);
  const [collapsedHunks, setCollapsedHunks] = useState<Set<number>>(new Set());
  const files = useAtomValue(activeSessionFilesAtom);
  const scrollRef = useRef<HTMLDivElement>(null);
  const hunkRefs = useRef<Map<number, HTMLDivElement>>(new Map());

  const fileTimestamp = files.find((f) => f.path === filePath)?.timestamp ?? 0;

  const fetchDiff = useCallback(() => {
    setLoading(true);
    setError(null);

    invoke<{ diff_text: string }>("get_file_diff", { sessionId, filePath })
      .then((res) => {
        const parsed = parseDiff(res.diff_text);
        setLines(parsed.lines);
        setHunks(parsed.hunks);
        setActiveHunk(0);
        setCollapsedHunks(new Set());
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [filePath, sessionId]);

  useEffect(() => {
    fetchDiff();
  }, [fetchDiff, fileTimestamp]);

  const navigateHunk = useCallback((direction: -1 | 1) => {
    setActiveHunk((prev) => {
      const next = Math.max(0, Math.min(hunks.length - 1, prev + direction));
      if (next === prev) return prev;
      const el = hunkRefs.current.get(next);
      if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
      return next;
    });
  }, [hunks.length]);

  const toggleCollapse = useCallback((index: number) => {
    setCollapsedHunks((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }, []);

  // Steal focus when the panel zone wrapper gets focused (Alt+Left/Right)
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    const panel = el.closest("[data-focus-zone='panel']") as HTMLElement | null;
    if (!panel) return;

    function handleFocus(e: FocusEvent) {
      if (e.target === panel) {
        scrollRef.current?.focus();
      }
    }

    panel.addEventListener("focus", handleFocus, true);
    return () => panel.removeEventListener("focus", handleFocus, true);
  });

  // Focus on mount
  useEffect(() => {
    scrollRef.current?.focus();
  }, [filePath]);

  // Keyboard: J/K and Alt+Up/Down for hunks, Space for collapse
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (hunks.length > 1) {
        if (e.code === "KeyK" || (e.code === "ArrowUp" && e.altKey)) {
          e.preventDefault();
          e.stopPropagation();
          navigateHunk(-1);
          return;
        }
        if (e.code === "KeyJ" || (e.code === "ArrowDown" && e.altKey)) {
          e.preventDefault();
          e.stopPropagation();
          navigateHunk(1);
          return;
        }
      }
      if (e.code === "Space") {
        e.preventDefault();
        toggleCollapse(activeHunk);
      }
    }

    // Capture phase to run before global shortcuts
    el.addEventListener("keydown", handleKeyDown, true);
    return () => el.removeEventListener("keydown", handleKeyDown, true);
  }, [hunks.length, activeHunk, navigateHunk, toggleCollapse]);

  if (loading && lines.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">Loading diff...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-red-400">{error}</span>
      </div>
    );
  }

  if (lines.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No changes</span>
      </div>
    );
  }

  const adds = lines.filter((l) => l.type === "add").length;
  const removes = lines.filter((l) => l.type === "remove").length;
  const relPath = relativePath(filePath);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* File header + hunk navigation */}
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 px-3 py-1.5">
        <Tooltip>
          <TooltipTrigger className="cursor-default min-w-0">
            <span className="block truncate text-[11px] font-medium text-foreground/80 font-mono">
              {relPath}
            </span>
          </TooltipTrigger>
          <TooltipContent side="bottom" className="max-w-md">
            <p className="font-mono text-xs break-all">{filePath}</p>
          </TooltipContent>
        </Tooltip>
        <div className="flex shrink-0 items-center gap-2 ml-2">
          <span className="text-[10px] text-green-400">+{adds}</span>
          <span className="text-[10px] text-red-400">-{removes}</span>
          {hunks.length > 1 && (
            <div className="flex items-center gap-0.5 ml-1">
              <span className="text-[10px] text-muted-foreground">
                {activeHunk + 1}/{hunks.length}
              </span>
              <button
                onClick={() => navigateHunk(-1)}
                disabled={activeHunk <= 0}
                className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary disabled:opacity-30 transition-colors"
                aria-label="Previous change (K)"
              >
                <ChevronUp size={12} />
              </button>
              <button
                onClick={() => navigateHunk(1)}
                disabled={activeHunk >= hunks.length - 1}
                className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary disabled:opacity-30 transition-colors"
                aria-label="Next change (J)"
              >
                <ChevronDown size={12} />
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Scrollable diff content — plain div for native scroll + keyboard */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto outline-none"
        tabIndex={-1}
      >
        <div className="font-mono text-[11px] leading-[18px]" role="region" aria-label="File diff">
          {sideBySide ? (
            <SideBySideContent
              lines={lines}
              activeHunk={activeHunk}
              collapsedHunks={collapsedHunks}
              hunkRefs={hunkRefs}
              setActiveHunk={setActiveHunk}
            />
          ) : (
            <UnifiedContent
              lines={lines}
              activeHunk={activeHunk}
              collapsedHunks={collapsedHunks}
              hunkRefs={hunkRefs}
              setActiveHunk={setActiveHunk}
            />
          )}
        </div>
      </div>
    </div>
  );
}
