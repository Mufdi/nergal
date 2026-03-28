import { useEffect, useRef } from "react";
import { useAtomValue, useSetAtom, useAtom } from "jotai";
import { activeAnnotationsAtom, removeAnnotationAtom, type Annotation } from "@/stores/annotations";
import { planSidebarTabAtom } from "@/stores/plan";
import { PlanListView } from "@/components/panel/PlanListView";
import { X } from "lucide-react";

const TYPE_BADGE: Record<Annotation["type"], { label: string; bg: string; text: string }> = {
  comment: { label: "COMMENT", bg: "bg-blue-500/15", text: "text-blue-400" },
  replace: { label: "REPLACE", bg: "bg-yellow-500/15", text: "text-yellow-400" },
  delete: { label: "DELETE", bg: "bg-red-500/15", text: "text-red-400" },
  insert: { label: "INSERT", bg: "bg-green-500/15", text: "text-green-400" },
};

function scrollToAnnotation(id: string) {
  // web-highlighter uses data-highlight-id attribute
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
  // Fallback: search by annotation id in any highlight-id containing the id
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

export function PlanSidebar() {
  const [tab, setTab] = useAtom(planSidebarTabAtom);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const removeAnnotation = useSetAtom(removeAnnotationAtom);

  // Auto-switch to annotations tab only when first annotation is added (0 → >0)
  const prevCountRef = useRef(annotations.length);
  useEffect(() => {
    if (prevCountRef.current === 0 && annotations.length > 0) {
      setTab("annotations");
    }
    prevCountRef.current = annotations.length;
  }, [annotations.length, setTab]);

  return (
    <div className="flex h-full flex-col">
      {/* Tab bar */}
      <div className="flex shrink-0 border-b border-border/50">
        <button
          type="button"
          onClick={() => setTab("files")}
          className={`flex-1 py-1.5 text-center text-[10px] font-medium uppercase tracking-wider transition-colors ${
            tab === "files" ? "text-foreground border-b-2 border-primary" : "text-muted-foreground hover:text-foreground"
          }`}
        >
          Files
        </button>
        <button
          type="button"
          onClick={() => setTab("annotations")}
          className={`flex-1 py-1.5 text-center text-[10px] font-medium uppercase tracking-wider transition-colors ${
            tab === "annotations" ? "text-foreground border-b-2 border-primary" : "text-muted-foreground hover:text-foreground"
          }`}
        >
          Annotations {annotations.length > 0 && (
            <span className="ml-1 inline-flex size-4 items-center justify-center rounded bg-primary/20 text-[9px] text-primary">
              {annotations.length}
            </span>
          )}
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {tab === "files" ? (
          <PlanListView />
        ) : (
          <div className="py-1">
            {annotations.length === 0 ? (
              <p className="px-3 py-6 text-center text-[10px] text-muted-foreground">
                No annotations yet.<br />
                Hover over plan elements to annotate.
              </p>
            ) : (
              <div className="space-y-1 px-1.5">
                {annotations.map((ann) => {
                  const badge = TYPE_BADGE[ann.type];
                  const isGlobal = ann.target === "[global]";
                  return (
                    <button
                      key={ann.id}
                      type="button"
                      onClick={() => { if (!isGlobal) scrollToAnnotation(ann.id); }}
                      className={`group relative w-full rounded-md border border-border/30 bg-secondary/20 p-2 text-left transition-colors ${isGlobal ? "" : "hover:bg-secondary/40 cursor-pointer"}`}
                    >
                      {/* Header: type badge + remove */}
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

                      {/* Target text */}
                      {!isGlobal && (
                        <div className="mb-1 rounded bg-background/50 px-2 py-1">
                          <p className={`text-[10px] text-foreground/70 line-clamp-3 ${ann.type === "delete" ? "line-through text-red-400/70" : ""}`}>
                            &ldquo;{ann.target}&rdquo;
                          </p>
                        </div>
                      )}

                      {/* Annotation content */}
                      {ann.content && (
                        <p className="text-[10px] text-foreground/90 leading-relaxed">
                          {ann.content}
                        </p>
                      )}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
