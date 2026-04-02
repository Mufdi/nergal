import { useState, useRef, useEffect } from "react";
import { useSetAtom, useAtomValue } from "jotai";
import { addAnnotationAtom, activeAnnotationsAtom, clearAnnotationsAtom, serializeAnnotations, type AnnotationType } from "@/stores/annotations";
import { toastsAtom } from "@/stores/toast";
import { Tooltip, TooltipProvider, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import type { DomMeta } from "@/lib/highlighter";
import { MessageSquare, Replace, Trash2, Copy, X } from "lucide-react";

// Quick labels inspired by plannotator
const QUICK_LABELS = [
  "Clarify this",
  "Too vague",
  "Verify this",
  "Give me an example",
  "Simplify this",
  "Consider alternatives",
  "Too risky",
  "Out of scope",
  "Break this down",
  "Nice approach",
] as const;

interface ToolbarPosition {
  top: number;
  left: number;
}

interface PlanAnnotationToolbarProps {
  position: ToolbarPosition | null;
  targetText: string;
  highlightId: string;
  startMeta: DomMeta;
  endMeta: DomMeta;
  mode: "pinpoint" | "selection";
  flipped?: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

const ACTION_HANDLERS = ["comment", "replace", "delete", "copy", "quicklabel", "close"] as const;

export function PlanAnnotationToolbar({ position, targetText, highlightId, startMeta, endMeta, flipped = false, onClose, onConfirm }: PlanAnnotationToolbarProps) {
  const [view, setView] = useState<"actions" | "quicklabel" | "comment" | "replace">("actions");
  const [inputValue, setInputValue] = useState("");
  const [focusedAction, setFocusedAction] = useState(0);
  const [selectedLabel, setSelectedLabel] = useState(0);
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const addToast = useSetAtom(toastsAtom);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const actionsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (view === "comment" || view === "replace") {
      inputRef.current?.focus();
    }
    if (view === "actions") {
      setFocusedAction(0);
      requestAnimationFrame(() => actionsRef.current?.focus());
    }
    if (view === "quicklabel") {
      setSelectedLabel(0);
    }
  }, [view]);

  // Quicklabel keyboard handler — window capture phase so focus doesn't matter
  useEffect(() => {
    if (view !== "quicklabel") return;

    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setView("actions");
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setSelectedLabel((prev) => (prev + 1) % QUICK_LABELS.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        e.stopImmediatePropagation();
        setSelectedLabel((prev) => (prev - 1 + QUICK_LABELS.length) % QUICK_LABELS.length);
        return;
      }
      if (e.key === "Enter") {
        e.preventDefault();
        e.stopImmediatePropagation();
        // Read current selectedLabel from the DOM since closure might be stale
        const items = document.querySelectorAll("[data-quicklabel-item]");
        const selected = Array.from(items).findIndex((el) => el.getAttribute("data-nav-selected") === "true");
        const idx = selected >= 0 ? selected : 0;
        handleQuickLabel(QUICK_LABELS[idx]);
        return;
      }
      const num = parseInt(e.key);
      if (num >= 1 && num <= 9 && num <= QUICK_LABELS.length) {
        e.preventDefault();
        e.stopImmediatePropagation();
        handleQuickLabel(QUICK_LABELS[num - 1]);
      } else if (e.key === "0" && QUICK_LABELS.length >= 10) {
        e.preventDefault();
        e.stopImmediatePropagation();
        handleQuickLabel(QUICK_LABELS[9]);
      }
    }

    window.addEventListener("keydown", handleKey, true);
    return () => window.removeEventListener("keydown", handleKey, true);
  }, [view]);

  if (!position) return null;

  function submitAnnotation(type: AnnotationType, content: string) {
    addAnnotation({
      type,
      target: targetText.slice(0, 80),
      content,
      startMeta,
      endMeta,
      highlightId,
    });
    setView("actions");
    setInputValue("");
    onConfirm();
  }

  function handleQuickLabel(label: string) {
    submitAnnotation("comment", label);
  }

  function handleDelete() {
    submitAnnotation("delete", "");
  }

  function handleCopy() {
    navigator.clipboard.writeText(targetText).then(() => {
      addToast({ message: "Copied", description: "Text copied to clipboard", type: "success" });
    }).catch(console.error);
    onClose();
  }

  function handleSubmit() {
    if (!inputValue.trim()) return;
    if (view === "comment") submitAnnotation("comment", inputValue);
    if (view === "replace") submitAnnotation("replace", inputValue);
  }

  const posStyle = flipped
    ? { top: position.top, left: position.left, transform: "translateY(-100%)" as const }
    : { top: position.top, left: position.left };

  function handleActionsKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowRight" || e.key === "Tab" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      setFocusedAction((prev) => (prev + 1) % ACTION_HANDLERS.length);
      return;
    }
    if (e.key === "ArrowLeft" || (e.key === "Tab" && e.shiftKey)) {
      e.preventDefault();
      e.stopPropagation();
      setFocusedAction((prev) => (prev - 1 + ACTION_HANDLERS.length) % ACTION_HANDLERS.length);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      const action = ACTION_HANDLERS[focusedAction];
      if (action === "comment") setView("comment");
      else if (action === "replace") setView("replace");
      else if (action === "delete") handleDelete();
      else if (action === "copy") handleCopy();
      else if (action === "quicklabel") setView("quicklabel");
      else if (action === "close") onClose();
      return;
    }
    // Number keys 1-4 for direct action access
    const num = parseInt(e.key);
    if (num >= 1 && num <= 4) {
      e.preventDefault();
      e.stopPropagation();
      if (num === 1) setView("comment");
      else if (num === 2) setView("replace");
      else if (num === 3) handleDelete();
      else if (num === 4) handleCopy();
    }
  }

  const actionClasses = (idx: number, base: string) =>
    `${base}${focusedAction === idx ? " ring-1 ring-ring" : ""}`;

  // Actions view — icon toolbar
  if (view === "actions") {
    return (
      <div
        ref={actionsRef}
        tabIndex={-1}
        onKeyDown={handleActionsKeyDown}
        className="fixed z-50 rounded-lg border border-border bg-popover shadow-lg outline-none"
        style={posStyle}
      >
        <TooltipProvider delay={0}>
          <div className="flex items-center gap-0 p-0.5">
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("comment")}
                className={actionClasses(0, "rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors")}
              >
                <MessageSquare className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Comment (1)</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("replace")}
                className={actionClasses(1, "rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors")}
              >
                <Replace className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Replace (2)</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={handleDelete}
                className={actionClasses(2, "rounded p-1.5 text-muted-foreground hover:bg-red-500/15 hover:text-red-400 transition-colors")}
              >
                <Trash2 className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Delete (3)</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={handleCopy}
                className={actionClasses(3, "rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors")}
              >
                <Copy className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Copy (4)</TooltipContent>
            </Tooltip>
            <div className="mx-0.5 h-4 w-px bg-border" />
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("quicklabel")}
                className={actionClasses(4, "rounded px-1.5 py-1 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors")}
              >
                ⚡
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Quick label</TooltipContent>
            </Tooltip>
            <div className="mx-0.5 h-4 w-px bg-border" />
            <Tooltip>
              <TooltipTrigger
                onClick={onClose}
                className={actionClasses(5, "rounded p-1.5 text-muted-foreground hover:text-foreground transition-colors")}
              >
                <X className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Dismiss</TooltipContent>
            </Tooltip>
          </div>
        </TooltipProvider>
      </div>
    );
  }

  // Quick label picker — compact, keyboard handled via window capture (see useEffect above)
  if (view === "quicklabel") {
    return (
      <div
        className="fixed z-50 w-40 rounded-md border border-border/60 bg-popover py-0.5 shadow-md outline-none"
        style={posStyle}
      >
        {QUICK_LABELS.map((label, i) => (
          <button
            key={label}
            type="button"
            data-quicklabel-item
            data-nav-item
            {...(selectedLabel === i ? { "data-nav-selected": "true" } : {})}
            onClick={() => handleQuickLabel(label)}
            className={`flex w-full items-center justify-between rounded-sm px-2 py-[3px] text-left text-[10px] transition-colors ${
              selectedLabel === i ? "bg-white/15 text-foreground font-medium" : "text-foreground/80 hover:bg-white/10"
            }`}
          >
            <span>{label}</span>
            <span className="text-[9px] text-muted-foreground/40">{i + 1}</span>
          </button>
        ))}
      </div>
    );
  }

  // Comment or Replace input view
  return (
    <div
      className="fixed z-50 w-72 rounded-lg border border-border bg-popover p-2 shadow-lg"
      style={posStyle}
    >
      <div className="mb-1.5 flex items-center justify-between">
        <span className="text-[10px] font-medium uppercase text-muted-foreground">{view}</span>
        <button type="button" onClick={() => { setView("actions"); setInputValue(""); }} className="text-muted-foreground hover:text-foreground">
          <X className="size-3" />
        </button>
      </div>
      <textarea
        ref={inputRef}
        value={inputValue}
        onChange={(e) => setInputValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.ctrlKey) { e.preventDefault(); handleSubmit(); }
        }}
        placeholder={view === "comment" ? "Your comment..." : "Replace with..."}
        className="mb-1.5 h-20 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring"
      />
      <button
        type="button"
        onClick={handleSubmit}
        disabled={!inputValue.trim()}
        className="w-full rounded bg-primary px-2 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
      >
        Add {view} (Ctrl+Enter)
      </button>
    </div>
  );
}

interface PlanAnnotationFooterProps {
  planPath: string;
  reviewStatus: "idle" | "pending_review" | "submitted";
  onRevise: (feedback: string) => void;
  onApprove: () => void;
  onAddGlobalComment?: (comment: string) => void;
}

export function PlanAnnotationFooter({ planPath, reviewStatus, onRevise, onApprove, onAddGlobalComment }: PlanAnnotationFooterProps) {
  const annotations = useAtomValue(activeAnnotationsAtom);
  const clearAll = useSetAtom(clearAnnotationsAtom);
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const [showGlobalInput, setShowGlobalInput] = useState(false);
  const [globalComment, setGlobalComment] = useState("");
  const globalInputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (showGlobalInput) globalInputRef.current?.focus();
  }, [showGlobalInput]);

  function handleRevise() {
    onRevise(serializeAnnotations(annotations, planPath));
    clearAll();
  }

  function handleGlobalComment() {
    if (!globalComment.trim()) return;
    addAnnotation({
      type: "comment",
      target: "[global]",
      content: globalComment.trim(),
      startMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
      endMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
    });
    if (onAddGlobalComment) onAddGlobalComment(globalComment.trim());
    setGlobalComment("");
    setShowGlobalInput(false);
  }

  const isPending = reviewStatus === "pending_review";
  const isSubmitted = reviewStatus === "submitted";

  return (
    <div className="border-t border-border">
      {isPending && showGlobalInput && (
        <div className="border-b border-border/50 p-2">
          <textarea
            ref={globalInputRef}
            value={globalComment}
            onChange={(e) => setGlobalComment(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && e.ctrlKey) { e.preventDefault(); handleGlobalComment(); } }}
            placeholder="General comment about this plan..."
            className="mb-1.5 h-16 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring"
          />
          <div className="flex items-center justify-end gap-1">
            <button type="button" onClick={() => { setShowGlobalInput(false); setGlobalComment(""); }} className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground">
              Cancel
            </button>
            <button
              type="button"
              onClick={handleGlobalComment}
              disabled={!globalComment.trim()}
              className="rounded bg-primary px-2 py-0.5 text-[10px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              Add (Ctrl+Enter)
            </button>
          </div>
        </div>
      )}
      <div className="flex items-center justify-between px-3 py-1.5">
        {isSubmitted ? (
          <span className="text-xs text-muted-foreground">Waiting for Claude to revise...</span>
        ) : isPending ? (
          <>
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">{annotations.length} annotation{annotations.length !== 1 ? "s" : ""}</span>
              {!showGlobalInput && (
                <button
                  type="button"
                  onClick={() => setShowGlobalInput(true)}
                  title="Add global comment"
                  className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground"
                >
                  <MessageSquare className="size-3" />
                  Comment
                </button>
              )}
              <button
                type="button"
                onClick={clearAll}
                className="text-[10px] text-muted-foreground hover:text-foreground"
              >
                Clear all
              </button>
            </div>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={onApprove}
                className="rounded border border-border px-2 py-0.5 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                Approve
              </button>
              <button
                type="button"
                onClick={handleRevise}
                className="rounded bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
              >
                Revise
              </button>
            </div>
          </>
        ) : (
          <span className="text-xs text-muted-foreground">Plan review complete</span>
        )}
      </div>
    </div>
  );
}
