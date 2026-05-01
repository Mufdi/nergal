import { useEffect, useState, useCallback } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import type { PrSummary } from "@/stores/git";
import { PrViewer } from "@/components/git/PrViewer";
import { Kbd } from "@/components/ui/kbd";
import { zenModeAtom, prZenAtom } from "@/stores/zenMode";
import { conflictsZenOpenAtom } from "@/stores/conflict";
import {
  ChevronLeft,
  Loader2,
  GitPullRequest,
  ExternalLink,
} from "lucide-react";

interface PrsChipProps {
  sessionId: string;
  workspaceId: string | null;
}

interface PrTabData {
  workspaceId: string;
  prNumber: number;
  title: string;
  state: string;
  url: string;
  baseRefName: string;
  headRefName: string;
  updatedAt: string;
}

function summaryToData(workspaceId: string, pr: PrSummary): PrTabData {
  return {
    workspaceId,
    prNumber: pr.number,
    title: pr.title,
    state: pr.state,
    url: pr.url,
    baseRefName: pr.base_ref_name,
    headRefName: pr.head_ref_name,
    updatedAt: pr.updated_at,
  };
}

export function PrsChip({ sessionId: _sessionId, workspaceId }: PrsChipProps) {
  const [prs, setPrs] = useState<PrSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [cursor, setCursor] = useState(0);
  const [selected, setSelected] = useState<PrSummary | null>(null);
  /// One-shot flag: PrsChip's Enter on a PR row sets `selected` *and* asks
  /// the viewer to open its file picker on mount. Cleared when the user goes
  /// back to the PR list. Without this flag the viewer always defaulted to
  /// the first file, which the user found surprising — they expect to pick.
  const [openPickerOnSelect, setOpenPickerOnSelect] = useState(false);
  const zenState = useAtomValue(zenModeAtom);
  const conflictsZen = useAtomValue(conflictsZenOpenAtom);
  const prZen = useAtomValue(prZenAtom);
  const setPrZen = useSetAtom(prZenAtom);
  // PRs chip never lives in Zen — bail when any overlay is open so j/k
  // doesn't slide the (hidden) picker cursor while the user navigates Zen.
  const listenerActive = !(zenState.open || conflictsZen || prZen !== null);

  // Ctrl+Shift+0 routes here when chipMode === "prs" (see shortcuts.ts).
  // Open PR Zen with the currently-selected PR; the cursor PR is the
  // fallback when the user pressed the shortcut from the picker without
  // picking a row first.
  useEffect(() => {
    function onExpand(ev: Event) {
      const detail = (ev as CustomEvent<{ workspaceId: string }>).detail;
      if (!detail || detail.workspaceId !== workspaceId) return;
      const target = selected ?? prs[cursor] ?? null;
      if (!target || !workspaceId) return;
      setPrZen(summaryToData(workspaceId, target));
    }
    document.addEventListener("cluihud:expand-zen-pr", onExpand);
    return () => document.removeEventListener("cluihud:expand-zen-pr", onExpand);
  }, [workspaceId, selected, prs, cursor, setPrZen]);

  const refresh = useCallback(() => {
    if (!workspaceId) { setLoading(false); return; }
    invoke<PrSummary[]>("list_prs", { workspaceId })
      .then((rows) => { setPrs(rows); setLoading(false); })
      .catch(() => setLoading(false));
  }, [workspaceId]);

  useEffect(() => { refresh(); }, [refresh]);

  useEffect(() => {
    if (cursor >= prs.length) setCursor(Math.max(0, prs.length - 1));
  }, [cursor, prs.length]);

  // Picker keyboard nav. Disabled when a PR is selected (viewer takes over).
  useEffect(() => {
    if (selected) return;
    if (prs.length === 0) return;
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
        setCursor((i) => (i + 1) % prs.length);
        return;
      }
      if (e.code === "ArrowUp" || e.code === "KeyK") {
        e.preventDefault();
        setCursor((i) => (i - 1 + prs.length) % prs.length);
        return;
      }
      if (e.code === "Enter") {
        const pr = prs[cursor];
        if (!pr) return;
        e.preventDefault();
        setSelected(pr);
        setOpenPickerOnSelect(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, prs, cursor, listenerActive]);

  // Backspace in viewer mode returns to picker. Captured at window level
  // because the viewer's scrollRef has tabIndex=-1 and handles its own keys.
  useEffect(() => {
    if (!selected) return;
    if (!listenerActive) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      const inField = target?.tagName === "INPUT"
        || target?.tagName === "TEXTAREA"
        || !!target?.closest(".cm-editor")
        || target?.getAttribute("contenteditable") === "true";
      if (inField) return;
      if (e.code === "Backspace" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setSelected(null);
        setOpenPickerOnSelect(false);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected, listenerActive]);

  if (!workspaceId) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[10px] text-muted-foreground">No workspace bound to this session</span>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center gap-2">
        <Loader2 size={14} className="animate-spin text-muted-foreground" />
        <span className="text-[11px] text-muted-foreground">Loading PRs...</span>
      </div>
    );
  }

  if (selected) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex shrink-0 items-center gap-1.5 border-b border-border/40 bg-secondary/20 px-2 py-1">
          <button
            onClick={() => { setSelected(null); setOpenPickerOnSelect(false); }}
            className="flex h-5 items-center gap-1 rounded text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
          >
            <ChevronLeft size={11} />
            All PRs
            <Kbd keys="backspace" />
          </button>
          <span className="text-[9px] text-muted-foreground/50">·</span>
          <span className="truncate text-[10px] font-mono text-muted-foreground">
            #{selected.number}
          </span>
        </div>
        <div className="flex-1 overflow-hidden">
          <PrViewer data={summaryToData(workspaceId, selected)} defaultPickerOpen={openPickerOnSelect} />
        </div>
      </div>
    );
  }

  if (prs.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <div className="text-center">
          <GitPullRequest size={20} className="mx-auto mb-2 text-muted-foreground/40" />
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
        <span className="text-[9px] text-muted-foreground/50">
          <Kbd keys="enter" /> open
        </span>
      </div>

      <div className="flex-1 overflow-y-auto">
        {prs.map((pr, i) => {
          const isCursor = cursor === i;
          return (
            <button
              key={pr.number}
              onMouseEnter={() => setCursor(i)}
              onClick={() => { setSelected(pr); setOpenPickerOnSelect(true); }}
              ref={(el) => { if (el && isCursor) el.scrollIntoView({ block: "nearest" }); }}
              className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors border-l-2 ${
                isCursor
                  ? "border-l-orange-500 bg-orange-500/10"
                  : "border-l-transparent hover:bg-secondary/30"
              }`}
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
              <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">
                {pr.title}
              </span>
              <span className="shrink-0 font-mono text-[9px] text-muted-foreground/60">
                {pr.head_ref_name} → {pr.base_ref_name}
              </span>
              <a
                href={pr.url}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
                className="shrink-0 text-muted-foreground/50 hover:text-foreground"
                title="Open on GitHub"
              >
                <ExternalLink size={10} />
              </a>
            </button>
          );
        })}
      </div>

      <div className="shrink-0 border-t border-border/40 px-3 py-1 text-[9px] text-muted-foreground/60">
        <Kbd keys="arrowup" /><Kbd keys="arrowdown" /> · <Kbd keys="j" /><Kbd keys="k" /> move · <Kbd keys="enter" /> open · <Kbd keys="backspace" /> back
      </div>
    </div>
  );
}
