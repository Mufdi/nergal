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

export function PlanAnnotationToolbar({ position, targetText, highlightId, startMeta, endMeta, flipped = false, onClose, onConfirm }: PlanAnnotationToolbarProps) {
  const [view, setView] = useState<"actions" | "quicklabel" | "comment" | "replace">("actions");
  const [inputValue, setInputValue] = useState("");
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const addToast = useSetAtom(toastsAtom);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (view === "comment" || view === "replace") {
      inputRef.current?.focus();
    }
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

  // Actions view — icon toolbar
  if (view === "actions") {
    return (
      <div
        className="fixed z-50 rounded-lg border border-border bg-popover shadow-lg"
        style={posStyle}
      >
        <TooltipProvider delay={0}>
          <div className="flex items-center gap-0 p-0.5">
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("comment")}
                className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                <MessageSquare className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Comment</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("replace")}
                className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                <Replace className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Replace</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={handleDelete}
                className="rounded p-1.5 text-muted-foreground hover:bg-red-500/15 hover:text-red-400 transition-colors"
              >
                <Trash2 className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Delete</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger
                onClick={handleCopy}
                className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                <Copy className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Copy</TooltipContent>
            </Tooltip>
            <div className="mx-0.5 h-4 w-px bg-border" />
            <Tooltip>
              <TooltipTrigger
                onClick={() => setView("quicklabel")}
                className="rounded px-1.5 py-1 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                ⚡
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Quick label</TooltipContent>
            </Tooltip>
            <div className="mx-0.5 h-4 w-px bg-border" />
            <Tooltip>
              <TooltipTrigger
                onClick={onClose}
                className="rounded p-1.5 text-muted-foreground hover:text-foreground transition-colors"
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

  // Quick label picker — compact, no emojis
  if (view === "quicklabel") {
    return (
      <div
        className="fixed z-50 w-40 rounded-md border border-border/60 bg-popover py-0.5 shadow-md"
        style={posStyle}
      >
        {QUICK_LABELS.map((label, i) => (
          <button
            key={label}
            type="button"
            onClick={() => handleQuickLabel(label)}
            className="flex w-full items-center justify-between px-2 py-[3px] text-left text-[10px] text-foreground/80 hover:bg-secondary transition-colors"
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
        onKeyDown={(e) => { if (e.key === "Enter" && e.ctrlKey) { e.preventDefault(); handleSubmit(); } }}
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
