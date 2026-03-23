import { useState, useCallback, useRef, type ReactNode, type ComponentPropsWithoutRef } from "react";
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
  const [toolbar, setToolbar] = useState<ToolbarState | null>(null);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const containerRef = useRef<HTMLDivElement>(null);
  const mouseDownPos = useRef<{ x: number; y: number } | null>(null);
  const activeElRef = useRef<HTMLElement | null>(null);

  const annotationMap = new Map<string, Annotation>();
  for (const ann of annotations) {
    annotationMap.set(ann.target, ann);
  }

  function getGutter(children: ReactNode): string {
    const text = extractText(children).slice(0, 80);
    const ann = annotationMap.get(text);
    return ann ? `border-l-2 ${GUTTER_COLORS[ann.type]} pl-2` : "";
  }

  // Track mouse down position to distinguish click from drag (text selection)
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    mouseDownPos.current = { x: e.clientX, y: e.clientY };
  }, []);

  const handleMouseUp = useCallback((e: React.MouseEvent) => {
    if (toolbar) return;

    const downPos = mouseDownPos.current;
    mouseDownPos.current = null;
    if (!downPos) return;

    const dx = Math.abs(e.clientX - downPos.x);
    const dy = Math.abs(e.clientY - downPos.y);
    const wasDrag = dx > 5 || dy > 5;

    // Small delay so browser finalizes selection before we read it
    setTimeout(() => {
      const sel = window.getSelection();
      const hasSelection = sel && !sel.isCollapsed && sel.toString().trim().length >= 2;

      if (hasSelection && containerRef.current?.contains(sel.anchorNode)) {
        // Text selection → show selection toolbar (don't clear selection)
        const range = sel.getRangeAt(0);
        const rect = range.getBoundingClientRect();
        setToolbar({
          position: { top: rect.bottom + 4, left: rect.left },
          targetText: sel.toString().trim(),
          targetRange: { start: 0, end: sel.toString().length },
          mode: "selection",
        });
        return;
      }

      if (wasDrag) return;

      // Clean click on annotatable element → pinpoint toolbar
      const target = (e.target as HTMLElement).closest("[data-annotatable]") as HTMLElement | null;
      if (!target) return;

      // Mark element as active (yellow dashed outline)
      activeElRef.current?.classList.remove("annotatable-active");
      target.classList.add("annotatable-active");
      activeElRef.current = target;

      const text = target.textContent ?? "";
      const rect = target.getBoundingClientRect();
      setToolbar({
        position: { top: rect.bottom + 4, left: rect.left },
        targetText: text,
        targetRange: { start: 0, end: text.length },
        mode: "pinpoint",
      });
    }, 10);
  }, [toolbar]);

  const closeToolbar = useCallback(() => {
    activeElRef.current?.classList.remove("annotatable-active");
    activeElRef.current = null;
    setToolbar((prev) => {
      if (prev?.mode !== "selection") {
        window.getSelection()?.removeAllRanges();
      }
      return null;
    });
  }, []);

  // Escape closes toolbar
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape" && toolbar) {
      e.preventDefault();
      closeToolbar();
    }
  }, [toolbar, closeToolbar]);

  return (
    <div
      ref={containerRef}
      className="annotatable-plan prose-invert max-w-none px-4 py-3 text-sm text-text"
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onKeyDown={handleKeyDown}
      tabIndex={-1}
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
            <ul data-annotatable="list" className={`annotatable-el mb-2 ml-4 list-disc text-text`} {...props} />
          ),
          ol: ({ node, ...props }: ComponentPropsWithoutRef<"ol"> & { node?: unknown }) => (
            <ol data-annotatable="list" className={`annotatable-el mb-2 ml-4 list-decimal text-text`} {...props} />
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
          pre: ({ node: _node, children }: ComponentPropsWithoutRef<"pre"> & { node?: unknown }) => <div>{children}</div>,
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
        <PlanAnnotationToolbar
          position={toolbar.position}
          targetText={toolbar.targetText}
          targetRange={toolbar.targetRange}
          mode={toolbar.mode}
          onClose={closeToolbar}
        />
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
