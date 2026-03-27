import { useState } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeAnnotationsAtom, removeAnnotationAtom, type Annotation } from "@/stores/annotations";
import { PlanListView } from "@/components/panel/PlanListView";
import { X } from "lucide-react";

type SidebarTab = "files" | "annotations";

const TYPE_BADGE: Record<Annotation["type"], { label: string; bg: string; text: string }> = {
  comment: { label: "COMMENT", bg: "bg-blue-500/15", text: "text-blue-400" },
  replace: { label: "REPLACE", bg: "bg-yellow-500/15", text: "text-yellow-400" },
  delete: { label: "DELETE", bg: "bg-red-500/15", text: "text-red-400" },
  insert: { label: "INSERT", bg: "bg-green-500/15", text: "text-green-400" },
};

function scrollToAnnotation(id: string) {
  const marks = document.querySelectorAll(`mark[data-bindid="${id}"], mark[data-bindid]`);
  for (const mark of marks) {
    // web-highlighter stores the id; check the parent too
    const highlighter = mark as HTMLElement;
    if (highlighter.dataset.bindid === id || highlighter.closest(`[data-bindid="${id}"]`)) {
      highlighter.scrollIntoView({ behavior: "smooth", block: "center" });
      // Brief flash effect
      highlighter.style.outline = "2px solid rgba(253, 224, 71, 0.8)";
      highlighter.style.outlineOffset = "2px";
      setTimeout(() => {
        highlighter.style.outline = "";
        highlighter.style.outlineOffset = "";
      }, 1500);
      return;
    }
  }
  // Fallback: search by annotation text content in annotatable elements
  const allMarks = document.querySelectorAll(".annotatable-plan mark");
  for (const mark of allMarks) {
    if (mark.textContent?.includes(id.slice(0, 10))) {
      (mark as HTMLElement).scrollIntoView({ behavior: "smooth", block: "center" });
      return;
    }
  }
}

export function PlanSidebar() {
  const [tab, setTab] = useState<SidebarTab>("files");
  const annotations = useAtomValue(activeAnnotationsAtom);
  const removeAnnotation = useSetAtom(removeAnnotationAtom);

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
