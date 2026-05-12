import { useRef, useEffect } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeAnnotationsAtom, removeAnnotationAtom, clearAnnotationsAtom, type Annotation } from "@/stores/annotations";
import { X, Trash2, MessageSquareDashed, MessageSquare } from "lucide-react";

const TYPE_BADGE: Record<Annotation["type"], { label: string; bg: string; text: string }> = {
  comment: { label: "COMMENT", bg: "bg-blue-500/15", text: "text-blue-400" },
  replace: { label: "REPLACE", bg: "bg-yellow-500/15", text: "text-yellow-400" },
  delete: { label: "DELETE", bg: "bg-red-500/15", text: "text-red-400" },
  insert: { label: "INSERT", bg: "bg-green-500/15", text: "text-green-400" },
};

function scrollToAnnotation(id: string) {
  const mark = document.querySelector(`.annotatable-plan mark[data-highlight-id="${id}"]`) as HTMLElement | null;
  if (mark) {
    mark.scrollIntoView({ behavior: "smooth", block: "center" });
    mark.style.outline = "2px solid #fde047";
    mark.style.outlineOffset = "2px";
    setTimeout(() => {
      mark.style.outline = "";
      mark.style.outlineOffset = "";
    }, 1500);
    return;
  }
  const allMarks = document.querySelectorAll(".annotatable-plan mark[data-highlight-id]");
  for (const m of allMarks) {
    const highlightId = (m as HTMLElement).dataset.highlightId ?? "";
    if (highlightId.includes(id) || id.includes(highlightId)) {
      (m as HTMLElement).scrollIntoView({ behavior: "smooth", block: "center" });
      (m as HTMLElement).style.outline = "2px solid #fde047";
      (m as HTMLElement).style.outlineOffset = "2px";
      setTimeout(() => {
        (m as HTMLElement).style.outline = "";
        (m as HTMLElement).style.outlineOffset = "";
      }, 1500);
      return;
    }
  }
}

interface AnnotationsDrawerProps {
  open: boolean;
  onToggle: () => void;
}

export function AnnotationsDrawer({ open, onToggle }: AnnotationsDrawerProps) {
  const annotations = useAtomValue(activeAnnotationsAtom);
  const removeAnnotation = useSetAtom(removeAnnotationAtom);
  const clearAnnotations = useSetAtom(clearAnnotationsAtom);
  const selectedIdxRef = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);

  // Focus the drawer when it opens — polls until the DOM element exists and receives focus
  useEffect(() => {
    if (!open || annotations.length === 0) return;
    let attempts = 0;
    const timer = setInterval(() => {
      attempts++;
      const el = containerRef.current;
      if (!el) { if (attempts > 20) clearInterval(timer); return; }
      el.focus();
      if (document.activeElement === el || el.contains(document.activeElement)) {
        const items = el.querySelectorAll("[data-nav-item]");
        for (const item of items) item.removeAttribute("data-nav-selected");
        if (items[0]) items[0].setAttribute("data-nav-selected", "true");
        selectedIdxRef.current = 0;
        clearInterval(timer);
      }
      if (attempts > 20) clearInterval(timer);
    }, 50);
    return () => clearInterval(timer);
  }, [open, annotations.length]);

  function getItems(container: HTMLElement): HTMLElement[] {
    return Array.from(container.querySelectorAll("[data-nav-item]"));
  }

  function updateSelection(container: HTMLElement, idx: number) {
    const items = getItems(container);
    for (const item of items) item.removeAttribute("data-nav-selected");
    if (items[idx]) {
      items[idx].setAttribute("data-nav-selected", "true");
      items[idx].scrollIntoView({ block: "nearest" });
    }
    selectedIdxRef.current = idx;
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.shiftKey) return;

    const container = e.currentTarget as HTMLElement;
    const items = getItems(container);
    if (items.length === 0) return;
    const idx = selectedIdxRef.current;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      updateSelection(container, Math.min(idx + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      updateSelection(container, Math.max(idx - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[idx]?.click();
    } else if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      const ann = annotations[idx];
      if (ann) removeAnnotation(ann.id);
    }
  }

  // Keep an empty placeholder mounted so annotation count transitions don't
  // trigger the WebKitGTK paint leak that mount/unmount cycles produce.
  // Zero height so the panel above gets the full column.
  if (annotations.length === 0) {
    return <div className="h-0 shrink-0" aria-hidden />;
  }

  if (!open) {
    return (
      <button
        onClick={onToggle}
        className="mt-1 flex h-7 shrink-0 items-center gap-1.5 rounded-lg border-2 border-border bg-card px-3 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
      >
        <MessageSquareDashed size={11} className="text-primary" />
        <span>{annotations.length} annotation{annotations.length !== 1 ? "s" : ""}</span>
      </button>
    );
  }

  return (
    <div
      ref={containerRef}
      className="mt-1 flex shrink-0 flex-col rounded-lg border-2 border-border bg-card outline-none"
      style={{ maxHeight: "30vh" }}
      data-annotations-drawer
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      {/* Header */}
      <div className="flex h-8 items-center justify-between border-b border-border px-3">
        <div className="flex items-center gap-2">
          <MessageSquareDashed size={12} className="text-primary" />
          <span className="text-xs font-medium text-foreground">Annotations</span>
          <span className="text-[10px] text-muted-foreground">({annotations.length})</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => document.dispatchEvent(new CustomEvent("cluihud:toggle-global-comment"))}
            className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
            title="Add global comment"
          >
            <MessageSquare size={12} />
          </button>
          <button
            onClick={() => clearAnnotations()}
            className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
            title="Clear all annotations"
          >
            <Trash2 size={12} />
          </button>
          <button
            onClick={onToggle}
            className="rounded p-0.5 text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto py-1.5 px-2">
        <div className="space-y-1.5">
          {annotations.map((ann) => {
            const badge = TYPE_BADGE[ann.type];
            const isGlobal = ann.target === "[global]";
            return (
              <button
                key={ann.id}
                type="button"
                data-nav-item
                onClick={() => { if (!isGlobal) scrollToAnnotation(ann.id); }}
                className={`group relative w-full rounded p-2 text-left transition-colors ${isGlobal ? "bg-secondary/30" : "bg-secondary/30 hover:bg-secondary/50 cursor-pointer"}`}
              >
                <div className="mb-1 flex items-center justify-between">
                  <span className={`rounded px-1.5 py-0.5 text-[9px] font-bold ${badge.bg} ${badge.text}`}>
                    {isGlobal ? "GLOBAL" : badge.label}
                  </span>
                  <span
                    role="button"
                    tabIndex={0}
                    onClick={(e) => { e.stopPropagation(); removeAnnotation(ann.id); }}
                    onKeyDown={(e) => { if (e.key === "Enter") { e.stopPropagation(); removeAnnotation(ann.id); } }}
                    className="flex size-4 items-center justify-center rounded opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-foreground transition-opacity"
                    aria-label="Remove"
                  >
                    <X className="size-3" />
                  </span>
                </div>

                {!isGlobal && (
                  <p className={`mb-1 text-[10px] text-foreground/60 line-clamp-2 ${ann.type === "delete" ? "line-through text-red-400/60" : ""}`}>
                    &ldquo;{ann.target}&rdquo;
                  </p>
                )}

                {ann.content && (
                  <p className="text-[10px] text-foreground/90 leading-relaxed line-clamp-2">
                    {ann.content}
                  </p>
                )}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
