import { useState, useEffect, useRef, useCallback, type ReactNode, type ComponentPropsWithoutRef } from "react";
import { useAtomValue } from "jotai";
import { activeAnnotationsAtom, type Annotation } from "@/stores/annotations";
import { PlanAnnotationToolbar } from "./PlanAnnotationToolbar";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface ToolbarState {
  position: { top: number; left: number };
  targetText: string;
  targetRange: { start: number; end: number };
  mode: "pinpoint" | "selection";
}

const GUTTER_COLORS: Record<Annotation["type"], string> = {
  comment: "border-l-blue-500",
  replace: "border-l-yellow-500",
  delete: "border-l-red-500",
  insert: "border-l-green-500",
};

interface Props {
  content: string;
}

export function AnnotatableMarkdownView({ content }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [toolbar, setToolbar] = useState<ToolbarState | null>(null);
  const toolbarOpenRef = useRef(false);
  const annotations = useAtomValue(activeAnnotationsAtom);

  toolbarOpenRef.current = !!toolbar;

  const annotationMap = new Map<string, Annotation>();
  for (const ann of annotations) {
    annotationMap.set(ann.target, ann);
  }

  function getGutter(children: ReactNode): string {
    const text = extractText(children).slice(0, 80);
    const ann = annotationMap.get(text);
    return ann ? `border-l-2 ${GUTTER_COLORS[ann.type]} pl-2` : "";
  }

  // Clear all DOM-level highlights
  function clearDomState() {
    const c = containerRef.current;
    if (!c) return;
    c.querySelector("[data-pinpoint-active]")?.removeAttribute("data-pinpoint-active");
    c.querySelectorAll("mark.pending-selection").forEach((el) => {
      const parent = el.parentNode;
      while (el.firstChild) parent?.insertBefore(el.firstChild, el);
      el.remove();
    });
  }

  const closeToolbar = useCallback(() => {
    clearDomState();
    setToolbar(null);
  }, []);

  // Click handler for pinpoint mode
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleClick(e: MouseEvent) {
      if (!container) return;

      // If toolbar is open, close it (unless clicking inside toolbar)
      if (toolbarOpenRef.current) {
        const toolbarEl = document.querySelector("[data-annotation-toolbar]");
        if (toolbarEl?.contains(e.target as Node)) return;
        closeToolbar();
        return;
      }

      // If there's a text selection, don't do pinpoint
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed && sel.toString().trim().length > 1) return;

      const target = (e.target as HTMLElement).closest("[data-annotatable]") as HTMLElement | null;
      if (!target || !container.contains(target)) return;

      // Clear previous, set new
      container.querySelector("[data-pinpoint-active]")?.removeAttribute("data-pinpoint-active");
      target.setAttribute("data-pinpoint-active", "");

      const text = target.textContent ?? "";
      const rect = target.getBoundingClientRect();
      setToolbar({
        position: { top: rect.bottom + 4, left: rect.left },
        targetText: text,
        targetRange: { start: 0, end: text.length },
        mode: "pinpoint",
      });
    }

    container.addEventListener("click", handleClick);
    return () => container.removeEventListener("click", handleClick);
  }, [closeToolbar]);

  // Selection handler
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    function handleMouseUp() {
      if (toolbarOpenRef.current) return;

      setTimeout(() => {
        const sel = window.getSelection();
        if (!sel || sel.isCollapsed || sel.toString().trim().length < 2) return;
        if (!sel.rangeCount || !container!.contains(sel.anchorNode)) return;

        const text = sel.toString().trim();
        const range = sel.getRangeAt(0);

        // Clear pinpoint
        container!.querySelector("[data-pinpoint-active]")?.removeAttribute("data-pinpoint-active");

        // Wrap selection in <mark> (DOM-direct, no React re-render needed for this)
        try {
          const mark = document.createElement("mark");
          mark.className = "pending-selection";
          range.surroundContents(mark);

          const rect = mark.getBoundingClientRect();
          sel.removeAllRanges();
          setToolbar({
            position: { top: rect.bottom + 4, left: rect.left },
            targetText: text,
            targetRange: { start: 0, end: text.length },
            mode: "selection",
          });
        } catch {
          // Multi-element selection — use range rect as fallback
          const rect = range.getBoundingClientRect();
          sel.removeAllRanges();
          setToolbar({
            position: { top: rect.bottom + 4, left: rect.left },
            targetText: text,
            targetRange: { start: 0, end: text.length },
            mode: "selection",
          });
        }
      }, 10);
    }

    container.addEventListener("mouseup", handleMouseUp);
    return () => container.removeEventListener("mouseup", handleMouseUp);
  }, []);

  // Escape
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

  // Restore existing annotations as <mark> in DOM
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // Small delay to ensure markdown has rendered
    const timer = setTimeout(() => {
      container.querySelectorAll("mark.annotation-mark").forEach((el) => {
        const parent = el.parentNode;
        while (el.firstChild) parent?.insertBefore(el.firstChild, el);
        el.remove();
      });

      for (const ann of annotations) {
        const range = findTextInContainer(container, ann.target);
        if (!range) continue;
        try {
          const mark = document.createElement("mark");
          mark.className = `annotation-mark annotation-${ann.type}`;
          mark.dataset.annotationId = ann.id;
          range.surroundContents(mark);
        } catch {
          // Skip multi-element ranges
        }
      }
    }, 50);

    return () => clearTimeout(timer);
  }, [annotations, content]);

  // Right panel tooltip instant
  useEffect(() => {
    // Set tooltip delay on right panel collapsed tooltips
  }, []);

  return (
    <div
      ref={containerRef}
      className="annotatable-plan prose-invert max-w-none px-4 py-3 text-sm text-text"
    >
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ node, ...props }: ComponentPropsWithoutRef<"h1"> & { node?: unknown }) => (
            <div data-annotatable="heading" className={`annotatable-el ${getGutter(props.children)}`}>
              <h1 className="mb-2 mt-4 border-b border-border pb-1 text-lg font-semibold text-text" {...props} />
            </div>
          ),
          h2: ({ node, ...props }: ComponentPropsWithoutRef<"h2"> & { node?: unknown }) => (
            <div data-annotatable="heading" className={`annotatable-el ${getGutter(props.children)}`}>
              <h2 className="mb-2 mt-3 text-base font-semibold text-text" {...props} />
            </div>
          ),
          h3: ({ node, ...props }: ComponentPropsWithoutRef<"h3"> & { node?: unknown }) => (
            <div data-annotatable="heading" className={`annotatable-el ${getGutter(props.children)}`}>
              <h3 className="mb-1 mt-2 text-sm font-semibold text-text" {...props} />
            </div>
          ),
          p: ({ node, ...props }: ComponentPropsWithoutRef<"p"> & { node?: unknown }) => (
            <div data-annotatable="paragraph" className={`annotatable-el ${getGutter(props.children)}`}>
              <p className="mb-2 leading-relaxed text-text" {...props} />
            </div>
          ),
          li: ({ node, ...props }: ComponentPropsWithoutRef<"li"> & { node?: unknown }) => (
            <li data-annotatable="list-item" className={`annotatable-el mb-0.5 text-text ${getGutter(props.children)}`} {...props} />
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
            <div data-annotatable="blockquote" className={`annotatable-el ${getGutter(props.children)}`}>
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

      {toolbar && (
        <div data-annotation-toolbar>
          <PlanAnnotationToolbar
            position={toolbar.position}
            targetText={toolbar.targetText}
            targetRange={toolbar.targetRange}
            mode={toolbar.mode}
            onClose={closeToolbar}
          />
        </div>
      )}
    </div>
  );
}

function extractText(children: ReactNode): string {
  if (typeof children === "string") return children;
  if (typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(extractText).join("");
  if (children && typeof children === "object" && "props" in children) {
    return extractText((children as { props: { children?: ReactNode } }).props.children);
  }
  return "";
}

function findTextInContainer(container: HTMLElement, searchText: string): Range | null {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, null);
  let node: Text | null;
  while ((node = walker.nextNode() as Text | null)) {
    const text = node.textContent ?? "";
    const idx = text.indexOf(searchText);
    if (idx !== -1) {
      const range = document.createRange();
      range.setStart(node, idx);
      range.setEnd(node, idx + searchText.length);
      return range;
    }
  }
  return null;
}
