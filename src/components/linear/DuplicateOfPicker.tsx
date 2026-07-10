import { useEffect, useMemo, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { invoke } from "@/lib/tauri";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { toastsAtom } from "@/stores/toast";
import {
  clearLinearOverlayEntry,
  linearDuplicateRequestAtom,
  linearIssuesAtom,
  linearOverlayAtom,
  setLinearOverlayEntry,
} from "@/stores/linear";

/// Resolves the "duplicate of" relation Linear requires before an issue can
/// move into a Duplicate-typed state (BUG-35). Raised by a state change that
/// failed with "missing duplicate relation"; on pick it calls
/// `linear_mark_issue_duplicate` (relation + state in one shot).
export function LinearDuplicateOfPicker() {
  const [request, setRequest] = useAtom(linearDuplicateRequestAtom);
  const issues = useAtomValue(linearIssuesAtom);
  const setOverlay = useSetAtom(linearOverlayAtom);
  const addToast = useSetAtom(toastsAtom);
  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!request) return;
    setQuery("");
    setActiveIdx(0);
    setSubmitting(false);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [request]);

  // Candidate canonical issues: same team, never the issue itself.
  const candidates = useMemo(() => {
    if (!request) return [];
    const q = query.trim().toLowerCase();
    return issues
      .filter((i) => i.teamId === request.teamId && i.id !== request.issueId)
      .filter((i) =>
        !q
          ? true
          : (i.identifier ?? "").toLowerCase().includes(q) || i.title.toLowerCase().includes(q),
      )
      .slice(0, 50);
  }, [issues, request, query]);

  useEffect(() => {
    setActiveIdx((prev) => Math.min(prev, Math.max(0, candidates.length - 1)));
  }, [candidates.length]);

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-opt-idx="${activeIdx}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIdx]);

  if (!request) return null;

  const subject = issues.find((i) => i.id === request.issueId);
  const subjectLabel = subject?.identifier ?? subject?.title ?? request.issueId;

  async function pick(canonicalId: string) {
    if (!request || submitting) return;
    setSubmitting(true);
    // Optimistic: the state moves as soon as the relation + update land.
    setLinearOverlayEntry(setOverlay, request.issueId, "state", request.stateId);
    try {
      await invoke("linear_mark_issue_duplicate", {
        issueId: request.issueId,
        canonicalIssueId: canonicalId,
        stateId: request.stateId,
      });
      setRequest(null);
    } catch (err) {
      clearLinearOverlayEntry(setOverlay, request.issueId, "state");
      addToast({ message: "Mark as duplicate failed", description: String(err), type: "error" });
      setSubmitting(false);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && setRequest(null)}>
      <DialogContent
        className="sm:max-w-lg"
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActiveIdx((p) => Math.min(p + 1, candidates.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActiveIdx((p) => Math.max(p - 1, 0));
          } else if (e.key === "Enter" && !submitting) {
            e.preventDefault();
            e.stopPropagation();
            const c = candidates[activeIdx];
            if (c) void pick(c.id);
          }
        }}
      >
        <DialogHeader>
          <DialogTitle className="truncate">Mark {subjectLabel} as duplicate</DialogTitle>
          <DialogDescription>
            Pick the issue this one duplicates — Linear needs it before the issue can move
            into the Duplicate state.
          </DialogDescription>
        </DialogHeader>

        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActiveIdx(0);
          }}
          placeholder="Search issues…"
          className="w-full rounded border border-border bg-secondary/30 px-2 py-1.5 text-sm text-foreground outline-none placeholder:text-muted-foreground/50 focus-visible:border-ring"
        />

        <div ref={listRef} className="flex max-h-[45vh] min-h-0 flex-col overflow-y-auto">
          {candidates.length === 0 ? (
            <div className="flex items-center justify-center py-6">
              <span className="text-xs text-muted-foreground">No matching issues</span>
            </div>
          ) : (
            candidates.map((c, idx) => (
              <button
                key={c.id}
                type="button"
                data-opt-idx={idx}
                onMouseEnter={() => setActiveIdx(idx)}
                onClick={() => void pick(c.id)}
                disabled={submitting}
                className={`flex items-center gap-2 px-2 py-1.5 text-left transition-colors ${
                  idx === activeIdx ? "bg-secondary text-foreground" : "text-foreground/80 hover:bg-secondary/50"
                }`}
              >
                {c.identifier && (
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground">{c.identifier}</span>
                )}
                <span className="truncate text-xs">{c.title}</span>
              </button>
            ))
          )}
        </div>

        <DialogFooter className="flex-nowrap gap-1.5">
          <span className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
            <Kbd keys="arrowup" />
            <Kbd keys="arrowdown" /> navigate <Kbd keys="enter" /> select <Kbd keys="esc" /> cancel
          </span>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
