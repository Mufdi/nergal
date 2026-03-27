import { useState, useEffect, useRef, useCallback, memo, type ComponentPropsWithoutRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { activeAnnotationsAtom, annotationMapAtom, type AnnotationType } from "@/stores/annotations";
import { activeSessionIdAtom } from "@/stores/workspace";
import { toastsAtom } from "@/stores/toast";
import { PlanAnnotationToolbar } from "./PlanAnnotationToolbar";
import { createHighlighter, createTextRange, resolvePinpointTarget, HighlightEvent, type Highlighter, type HighlightSource, type DomMeta } from "@/lib/highlighter";
import { invoke } from "@/lib/tauri";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface ToolbarState {
  position: { top: number; left: number };
  targetText: string;
  highlightId: string;
  startMeta: DomMeta;
  endMeta: DomMeta;
  mode: "pinpoint" | "selection";
}

const ANNOTATION_TYPE_CLASSES: Record<AnnotationType, string> = {
  comment: "annotation-comment",
  replace: "annotation-replace",
  delete: "annotation-delete",
  insert: "annotation-insert",
};


interface Props {
  content: string;
}

/**
 * Memoized Markdown renderer — only re-renders when `content` changes.
 * This prevents web-highlighter DOM marks from being destroyed by React re-renders
 * triggered by annotation state changes.
 */
const PlanMarkdown = memo(function PlanMarkdown({ content }: { content: string }) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      components={{
        h1: ({ node, ...props }: ComponentPropsWithoutRef<"h1"> & { node?: unknown }) => (
          <div data-annotatable="heading" className="annotatable-el">
            <h1 className="mb-2 mt-4 border-b border-border pb-1 text-lg font-semibold text-text" {...props} />
          </div>
        ),
        h2: ({ node, ...props }: ComponentPropsWithoutRef<"h2"> & { node?: unknown }) => (
          <div data-annotatable="heading" className="annotatable-el">
            <h2 className="mb-2 mt-3 text-base font-semibold text-text" {...props} />
          </div>
        ),
        h3: ({ node, ...props }: ComponentPropsWithoutRef<"h3"> & { node?: unknown }) => (
          <div data-annotatable="heading" className="annotatable-el">
            <h3 className="mb-1 mt-2 text-sm font-semibold text-text" {...props} />
          </div>
        ),
        p: ({ node, ...props }: ComponentPropsWithoutRef<"p"> & { node?: unknown }) => (
          <div data-annotatable="paragraph" className="annotatable-el">
            <p className="mb-2 leading-relaxed text-text" {...props} />
          </div>
        ),
        li: ({ node, ...props }: ComponentPropsWithoutRef<"li"> & { node?: unknown }) => (
          <li data-annotatable="list-item" className="annotatable-el mb-0.5 text-text" {...props} />
        ),
        ul: ({ node, ...props }: ComponentPropsWithoutRef<"ul"> & { node?: unknown }) => (
          <ul data-annotatable="list" className="annotatable-el mb-2 ml-4 list-disc text-text" {...props} />
        ),
        ol: ({ node, ...props }: ComponentPropsWithoutRef<"ol"> & { node?: unknown }) => (
          <ol data-annotatable="list" className="annotatable-el mb-2 ml-4 list-decimal text-text" {...props} />
        ),
        code: ({ className, children, node, ...props }: ComponentPropsWithoutRef<"code"> & { node?: unknown }) => {
          const isBlock = className?.startsWith("language-");
          if (isBlock) {
            return (
              <div data-annotatable="code-block" className="annotatable-el">
                <pre className="my-2 overflow-x-auto bg-surface p-3 font-mono text-xs text-text">
                  <code className={className} {...props}>{children}</code>
                </pre>
              </div>
            );
          }
          return <code className="bg-surface-raised px-1 py-0.5 font-mono text-xs text-accent" {...props}>{children}</code>;
        },
        pre: ({ node: _n, children }: ComponentPropsWithoutRef<"pre"> & { node?: unknown }) => <div>{children}</div>,
        a: ({ node, ...props }: ComponentPropsWithoutRef<"a"> & { node?: unknown }) => <a className="text-accent underline" {...props} />,
        blockquote: ({ node, ...props }: ComponentPropsWithoutRef<"blockquote"> & { node?: unknown }) => (
          <div data-annotatable="blockquote" className="annotatable-el">
            <blockquote className="my-2 border-l-2 border-accent pl-3 text-text-muted" {...props} />
          </div>
        ),
        table: ({ node, ...props }: ComponentPropsWithoutRef<"table"> & { node?: unknown }) => (
          <table className="my-2 w-full border-collapse text-xs" {...props} />
        ),
        th: ({ node, ...props }: ComponentPropsWithoutRef<"th"> & { node?: unknown }) => (
          <th className="border border-border bg-surface-raised px-2 py-1 text-left font-medium" {...props} />
        ),
        td: ({ node, ...props }: ComponentPropsWithoutRef<"td"> & { node?: unknown }) => (
          <td data-annotatable="table-cell" className="annotatable-el border border-border px-2 py-1" {...props} />
        ),
      }}
    >
      {content}
    </Markdown>
  );
});

export function AnnotatableMarkdownView({ content }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const highlighterRef = useRef<Highlighter | null>(null);
  const [toolbar, setToolbar] = useState<ToolbarState | null>(null);
  const toolbarOpenRef = useRef(false);
  const pendingSourceRef = useRef<HighlightSource | null>(null);
  const hoverTargetRef = useRef<HTMLElement | null>(null);
  const pinpointTargetRef = useRef<HTMLElement | null>(null);
  const justCreatedRef = useRef(false);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const setAnnotationMap = useSetAtom(annotationMapAtom);
  const addToast = useSetAtom(toastsAtom);
  const loadedRef = useRef<string | null>(null);
  const prevContentRef = useRef(content);

  toolbarOpenRef.current = !!toolbar;

  // Close toolbar AND remove pending highlight (cancel/dismiss)
  const closeToolbar = useCallback(() => {
    if (pendingSourceRef.current && highlighterRef.current) {
      highlighterRef.current.remove(pendingSourceRef.current.id);
      pendingSourceRef.current = null;
    }
    if (pinpointTargetRef.current) {
      pinpointTargetRef.current.removeAttribute("data-pinpoint-active");
      pinpointTargetRef.current = null;
    }
    if (hoverTargetRef.current) {
      hoverTargetRef.current.removeAttribute("data-pinpoint-hover");
      hoverTargetRef.current = null;
    }
    setToolbar(null);
  }, []);

  // Close toolbar but KEEP the highlight (annotation confirmed)
  const confirmToolbar = useCallback(() => {
    if (pinpointTargetRef.current) {
      pinpointTargetRef.current.removeAttribute("data-pinpoint-active");
      pinpointTargetRef.current = null;
    }
    pendingSourceRef.current = null;
    setToolbar(null);
  }, []);

  // Initialize web-highlighter
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const highlighter = createHighlighter(container);
    highlighterRef.current = highlighter;

    highlighter.on(HighlightEvent.CREATE, ({ sources }) => {
      const source = sources[0];
      if (!source) return;

      // Remove previous pending highlight if any (prevents ghost marks on re-selection)
      if (pendingSourceRef.current) {
        try { highlighter.remove(pendingSourceRef.current.id); } catch { /* already gone */ }
      }

      pendingSourceRef.current = source;
      // Prevent the click event (which fires after mouseup) from immediately closing
      justCreatedRef.current = true;
      requestAnimationFrame(() => { justCreatedRef.current = false; });

      const doms = highlighter.getDoms(source.id);
      if (doms.length === 0) return;
      const rect = doms[0].getBoundingClientRect();

      setToolbar({
        position: { top: rect.bottom + 4, left: rect.left },
        targetText: source.text,
        highlightId: source.id,
        startMeta: source.startMeta,
        endMeta: source.endMeta,
        mode: "selection",
      });
    });

    const stopAutoHighlight = highlighter.run();

    return () => {
      stopAutoHighlight();
      highlighter.dispose();
      highlighterRef.current = null;
    };
  }, [content]);

  // Pinpoint click handler
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleClick(e: MouseEvent) {
      if (!container) return;

      // If toolbar just opened via CREATE (mouseup → CREATE → click), skip
      if (justCreatedRef.current) return;

      if (toolbarOpenRef.current) {
        const toolbarEl = document.querySelector("[data-annotation-toolbar]");
        if (toolbarEl?.contains(e.target as Node)) return;
        closeToolbar();
        return;
      }

      // If there's a text selection, let web-highlighter handle it via CREATE
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed && sel.toString().trim().length > 1) return;

      const target = resolvePinpointTarget(e.target, e);
      if (!target || !container.contains(target)) return;

      const range = createTextRange(target);
      if (!highlighterRef.current) return;

      // Clear hover
      if (hoverTargetRef.current) {
        hoverTargetRef.current.removeAttribute("data-pinpoint-hover");
        hoverTargetRef.current = null;
      }

      // Clear previous pinpoint
      if (pinpointTargetRef.current) {
        pinpointTargetRef.current.removeAttribute("data-pinpoint-active");
      }

      // Set yellow dashed outline on the annotatable element
      target.setAttribute("data-pinpoint-active", "");
      pinpointTargetRef.current = target;

      const source = highlighterRef.current.fromRange(range);
      pendingSourceRef.current = source;

      const doms = highlighterRef.current.getDoms(source.id);
      if (doms.length === 0) return;
      const rect = target.getBoundingClientRect();

      setToolbar({
        position: { top: rect.bottom + 4, left: rect.left },
        targetText: source.text,
        highlightId: source.id,
        startMeta: source.startMeta,
        endMeta: source.endMeta,
        mode: "pinpoint",
      });
    }

    container.addEventListener("click", handleClick);
    return () => container.removeEventListener("click", handleClick);
  }, [closeToolbar, content]);

  // Hover via mousemove + data-pinpoint-hover attribute
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleMouseMove(e: MouseEvent) {
      if (toolbarOpenRef.current) return;

      const target = resolvePinpointTarget(e.target, e);
      if (target === hoverTargetRef.current) return;

      if (hoverTargetRef.current) {
        hoverTargetRef.current.removeAttribute("data-pinpoint-hover");
      }

      if (target && container!.contains(target)) {
        target.setAttribute("data-pinpoint-hover", "");
        hoverTargetRef.current = target;
      } else {
        hoverTargetRef.current = null;
      }
    }

    function handleMouseLeave() {
      if (hoverTargetRef.current) {
        hoverTargetRef.current.removeAttribute("data-pinpoint-hover");
        hoverTargetRef.current = null;
      }
    }

    container.addEventListener("mousemove", handleMouseMove);
    container.addEventListener("mouseleave", handleMouseLeave);
    return () => {
      container.removeEventListener("mousemove", handleMouseMove);
      container.removeEventListener("mouseleave", handleMouseLeave);
    };
  }, [content]);

  // Escape handler
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape" && toolbarOpenRef.current) {
        e.preventDefault();
        closeToolbar();
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [closeToolbar]);

  // Restore persisted annotations + apply gutter indicators via DOM
  useEffect(() => {
    const highlighter = highlighterRef.current;
    const container = containerRef.current;
    if (!highlighter || !container) return;

    const timer = setTimeout(() => {
      // Clear previous gutters
      container.querySelectorAll("[data-gutter-types]").forEach((el) => {
        (el as HTMLElement).style.boxShadow = "";
        (el as HTMLElement).style.paddingLeft = "";
        el.removeAttribute("data-gutter-types");
      });

      // Collect annotation types per annotatable element
      const gutterMap = new Map<Element, Set<AnnotationType>>();

      for (const ann of annotations) {
        // Skip global comments — no DOM target
        if (ann.target === "[global]") continue;

        // Restore highlight marks via web-highlighter
        try {
          const existing = highlighter.getDoms(ann.id);
          if (existing.length === 0) {
            highlighter.fromStore(ann.startMeta, ann.endMeta, ann.target, ann.id);
          }
          highlighter.addClass(ANNOTATION_TYPE_CLASSES[ann.type], ann.id);
        } catch {
          // DOM structure changed — can't restore visual highlight
        }

        // Collect gutter types per element
        const doms = highlighter.getDoms(ann.id);
        if (doms.length > 0) {
          const annotatableEl = doms[0].closest("[data-annotatable]");
          if (annotatableEl) {
            if (!gutterMap.has(annotatableEl)) gutterMap.set(annotatableEl, new Set());
            gutterMap.get(annotatableEl)!.add(ann.type);
          }
        }
      }

      // Apply multi-color gutter via inset box-shadow
      const GUTTER_CSS_COLORS: Record<AnnotationType, string> = {
        comment: "rgb(59, 130, 246)",
        replace: "rgb(234, 179, 8)",
        delete: "rgb(239, 68, 68)",
        insert: "rgb(34, 197, 94)",
      };

      for (const [el, types] of gutterMap) {
        const htmlEl = el as HTMLElement;
        const typeArr = [...types];
        const lineWidth = 3;
        const shadows = typeArr.map((type, i) => {
          const offset = -(i * lineWidth + lineWidth);
          return `inset ${offset}px 0 0 0 ${GUTTER_CSS_COLORS[type]}`;
        });
        htmlEl.style.boxShadow = shadows.join(", ");
        htmlEl.style.paddingLeft = `${typeArr.length * lineWidth + 4}px`;
        htmlEl.setAttribute("data-gutter-types", typeArr.join(","));
      }
    }, 50);

    return () => clearTimeout(timer);
  }, [annotations, content]);

  // Load annotations from SQLite on mount / session change
  useEffect(() => {
    if (!sessionId || loadedRef.current === sessionId) return;
    loadedRef.current = sessionId;

    invoke("get_annotations", { sessionId }).then((rows: unknown) => {
      const loaded = (rows as Array<{
        id: string;
        ann_type: string;
        target: string;
        content: string;
        start_meta: string;
        end_meta: string;
      }>).map((r) => ({
        id: r.id,
        type: r.ann_type as AnnotationType,
        target: r.target,
        content: r.content,
        startMeta: JSON.parse(r.start_meta || "{}"),
        endMeta: JSON.parse(r.end_meta || "{}"),
      }));
      setAnnotationMap((prev) => ({ ...prev, [sessionId]: loaded }));
    }).catch(console.error);
  }, [sessionId, setAnnotationMap]);

  // Stale annotations toast
  useEffect(() => {
    if (prevContentRef.current !== content && annotations.length > 0) {
      addToast({
        message: "Plan updated",
        description: "Annotations may be stale. Clear them if they no longer apply.",
        type: "info",
      });
    }
    prevContentRef.current = content;
  }, [content, annotations.length, addToast]);

  return (
    <div
      ref={containerRef}
      className="annotatable-plan prose-invert max-w-none px-4 py-3 text-sm text-text"
    >
      <PlanMarkdown content={content} />

      {toolbar && (
        <div data-annotation-toolbar>
          <PlanAnnotationToolbar
            position={toolbar.position}
            targetText={toolbar.targetText}
            startMeta={toolbar.startMeta}
            endMeta={toolbar.endMeta}
            mode={toolbar.mode}
            onClose={closeToolbar}
            onConfirm={confirmToolbar}
          />
        </div>
      )}
    </div>
  );
}
