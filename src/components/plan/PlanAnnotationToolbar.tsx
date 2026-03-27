import { useState, useRef, useEffect } from "react";
import { useSetAtom, useAtomValue } from "jotai";
import { addAnnotationAtom, activeAnnotationsAtom, clearAnnotationsAtom, serializeAnnotations, type AnnotationType } from "@/stores/annotations";
import { toastsAtom } from "@/stores/toast";
import { Tooltip, TooltipProvider, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";
import type { DomMeta } from "@/lib/highlighter";
import { MessageSquare, Replace, Trash2, Plus, Copy, X } from "lucide-react";

interface ToolbarPosition {
  top: number;
  left: number;
}

interface PlanAnnotationToolbarProps {
  position: ToolbarPosition | null;
  targetText: string;
  startMeta: DomMeta;
  endMeta: DomMeta;
  mode: "pinpoint" | "selection";
  onClose: () => void;
  onConfirm: () => void;
}

export function PlanAnnotationToolbar({ position, targetText, startMeta, endMeta, onClose, onConfirm }: PlanAnnotationToolbarProps) {
  const [activeAction, setActiveAction] = useState<AnnotationType | null>(null);
  const [inputValue, setInputValue] = useState("");
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const addToast = useSetAtom(toastsAtom);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (activeAction) {
      inputRef.current?.focus();
    }
  }, [activeAction]);

  if (!position) return null;

  function handleSubmit() {
    if (!activeAction) return;
    addAnnotation({
      type: activeAction,
      target: targetText.slice(0, 80),
      content: inputValue,
      startMeta,
      endMeta,
    });
    setActiveAction(null);
    setInputValue("");
    onConfirm();
  }

  function handleDelete() {
    addAnnotation({
      type: "delete",
      target: targetText.slice(0, 80),
      content: "",
      startMeta,
      endMeta,
    });
    onConfirm();
  }

  function handleCopy() {
    navigator.clipboard.writeText(targetText).then(() => {
      addToast({ message: "Copied", description: "Text copied to clipboard", type: "success" });
    }).catch(console.error);
    onClose();
  }

  function handleAction(type: AnnotationType) {
    setActiveAction(type);
    setInputValue(type === "replace" ? targetText : "");
  }

  const actions: Array<{ type: AnnotationType; icon: typeof MessageSquare; label: string }> = [
    { type: "comment", icon: MessageSquare, label: "Comment" },
    { type: "replace", icon: Replace, label: "Replace" },
    { type: "insert", icon: Plus, label: "Insert" },
  ];

  return (
    <div
      className="fixed z-50 rounded-lg border border-border bg-popover shadow-lg"
      style={{ top: position.top, left: position.left }}
    >
      {!activeAction ? (
        <TooltipProvider delay={0}>
          <div className="flex items-center gap-0 p-0.5">
            {actions.map(({ type, icon: Icon, label }) => (
              <Tooltip key={type}>
                <TooltipTrigger
                  onClick={() => handleAction(type)}
                  className="rounded p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                >
                  <Icon className="size-3.5" />
                </TooltipTrigger>
                <TooltipContent side="bottom" sideOffset={4}>{label}</TooltipContent>
              </Tooltip>
            ))}
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
                onClick={onClose}
                className="rounded p-1.5 text-muted-foreground hover:text-foreground transition-colors"
              >
                <X className="size-3.5" />
              </TooltipTrigger>
              <TooltipContent side="bottom" sideOffset={4}>Dismiss</TooltipContent>
            </Tooltip>
          </div>
        </TooltipProvider>
      ) : (
        <div className="w-72 p-2">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[10px] font-medium uppercase text-muted-foreground">{activeAction}</span>
            <button type="button" onClick={() => { setActiveAction(null); setInputValue(""); }} className="text-muted-foreground hover:text-foreground">
              <X className="size-3" />
            </button>
          </div>
          <textarea
            ref={inputRef}
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && e.ctrlKey) { e.preventDefault(); handleSubmit(); } }}
            placeholder={activeAction === "comment" ? "Your comment..." : activeAction === "replace" ? "Replace with..." : "Content to insert..."}
            className="mb-1.5 h-20 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring"
          />
          <button
            type="button"
            onClick={handleSubmit}
            disabled={!inputValue.trim()}
            className="w-full rounded bg-primary px-2 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Add {activeAction} (Ctrl+Enter)
          </button>
        </div>
      )}
    </div>
  );
}

interface PlanAnnotationFooterProps {
  planPath: string;
  onRevise: (feedback: string) => void;
  onAddGlobalComment?: (comment: string) => void;
}

export function PlanAnnotationFooter({ planPath, onRevise, onAddGlobalComment }: PlanAnnotationFooterProps) {
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

  return (
    <div className="border-t border-border">
      {showGlobalInput && (
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
            onClick={clearAll}
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
      </div>
    </div>
  );
}
