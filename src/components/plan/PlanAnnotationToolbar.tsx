import { useState, useRef, useEffect } from "react";
import { useSetAtom, useAtomValue } from "jotai";
import { addAnnotationAtom, activeAnnotationsAtom, clearAnnotationsAtom, serializeAnnotations, type AnnotationType } from "@/stores/annotations";
import { MessageSquare, Replace, Trash2, Plus, X } from "lucide-react";

interface ToolbarPosition {
  top: number;
  left: number;
}

interface PlanAnnotationToolbarProps {
  position: ToolbarPosition | null;
  targetText: string;
  targetRange: { start: number; end: number };
  mode: "pinpoint" | "selection";
  onClose: () => void;
}

export function PlanAnnotationToolbar({ position, targetText, targetRange, mode, onClose }: PlanAnnotationToolbarProps) {
  const [activeAction, setActiveAction] = useState<AnnotationType | null>(null);
  const [inputValue, setInputValue] = useState("");
  const addAnnotation = useSetAtom(addAnnotationAtom);
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
      position: targetRange,
    });
    setActiveAction(null);
    setInputValue("");
    onClose();
  }

  function handleAction(type: AnnotationType) {
    if (type === "delete") {
      setActiveAction("delete");
      setInputValue("");
    } else {
      setActiveAction(type);
      setInputValue(type === "replace" ? targetText : "");
    }
  }

  const actions = mode === "pinpoint"
    ? [
        { type: "comment" as const, icon: MessageSquare, label: "Comment" },
        { type: "replace" as const, icon: Replace, label: "Replace" },
        { type: "delete" as const, icon: Trash2, label: "Delete" },
        { type: "insert" as const, icon: Plus, label: "Insert" },
      ]
    : [
        { type: "comment" as const, icon: MessageSquare, label: "Comment" },
        { type: "replace" as const, icon: Replace, label: "Replace" },
      ];

  return (
    <div
      className="fixed z-50 rounded-lg border border-border bg-popover shadow-lg"
      style={{ top: position.top, left: position.left }}
    >
      {!activeAction ? (
        <div className="flex items-center gap-0.5 p-1">
          {actions.map(({ type, icon: Icon, label }) => (
            <button
              key={type}
              type="button"
              onClick={() => handleAction(type)}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
            >
              <Icon className="size-3" />
              {label}
            </button>
          ))}
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-muted-foreground hover:text-foreground"
          >
            <X className="size-3" />
          </button>
        </div>
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
            placeholder={activeAction === "delete" ? "Reason for deletion..." : activeAction === "comment" ? "Your comment..." : "New content..."}
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

/// Annotation count bar + approve/revise buttons for the plan footer.
export function PlanAnnotationFooter({ planPath, onRevise }: { planPath: string; onRevise: (feedback: string) => void }) {
  const annotations = useAtomValue(activeAnnotationsAtom);
  const clearAll = useSetAtom(clearAnnotationsAtom);

  if (annotations.length === 0) return null;

  function handleRevise() {
    onRevise(serializeAnnotations(annotations, planPath));
    clearAll();
  }

  return (
    <div className="flex items-center justify-between border-t border-border px-3 py-1.5">
      <div className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground">{annotations.length} annotation{annotations.length !== 1 ? "s" : ""}</span>
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
  );
}
