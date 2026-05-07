import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { X } from "lucide-react";

const ANIMATION_MS = 100;

/// Generic floating panel chrome: draggable header, resizable corners, semi-
/// transparent island background, persistent geometry keyed by `panelId`.
/// Built load-bearing from day one — `panelId` keys the SQLite row so adding
/// a second floating tool is geometry-only, no schema migration.
///
/// Persistence is delegated to the parent via `geometry` + `onGeometryChange`
/// so the same wrapper can be wired to different storage backends (right
/// now: scratchpad. Future: any other floating tool with its own store).

export interface FloatingGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface FloatingPanelProps {
  /// Stable identifier for persistence. Each tool gets its own.
  panelId: string;
  open: boolean;
  onClose: () => void;
  geometry: FloatingGeometry;
  onGeometryChange: (next: FloatingGeometry) => void;
  /// 0..1 background opacity. Applied to the `--card` token.
  opacity: number;
  title?: React.ReactNode;
  children: React.ReactNode;
  /// Optional toolbar node injected to the right of the title.
  toolbar?: React.ReactNode;
  /// Minimum dimensions during resize.
  minWidth?: number;
  minHeight?: number;
  /// Override the wrapper's z-index (default 40). Use a higher value for
  /// utility panels that must always sit above other floating overlays
  /// (e.g. scratchpad on top of the browser host iframe).
  zIndex?: number;
}

const DEFAULT_MIN_WIDTH = 320;
const DEFAULT_MIN_HEIGHT = 200;

type DragMode = null | "move" | "resize-se" | "resize-sw" | "resize-ne" | "resize-nw";

interface DragSession {
  mode: DragMode;
  startX: number;
  startY: number;
  startGeo: FloatingGeometry;
}

export function FloatingPanel({
  panelId,
  open,
  onClose,
  geometry,
  onGeometryChange,
  opacity,
  title,
  children,
  toolbar,
  minWidth = DEFAULT_MIN_WIDTH,
  minHeight = DEFAULT_MIN_HEIGHT,
  zIndex = 40,
}: FloatingPanelProps) {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<DragSession | null>(null);
  const [draftGeo, setDraftGeo] = useState<FloatingGeometry | null>(null);
  const [mounted, setMounted] = useState(open);
  const [visible, setVisible] = useState(false);

  // Show: mount immediately, then flip `visible` on the next frame so the
  // CSS transition starts from the closed state.
  // Hide: flip `visible` to false (start exit anim), unmount after duration.
  useLayoutEffect(() => {
    if (open) {
      setMounted(true);
      const id = requestAnimationFrame(() => setVisible(true));
      return () => cancelAnimationFrame(id);
    }
    setVisible(false);
    const t = setTimeout(() => setMounted(false), ANIMATION_MS);
    return () => clearTimeout(t);
  }, [open]);

  const effective = draftGeo ?? geometry;

  const onPointerMove = useCallback((e: PointerEvent) => {
    const session = dragRef.current;
    if (!session) return;
    e.preventDefault();
    const dx = e.clientX - session.startX;
    const dy = e.clientY - session.startY;
    const next = { ...session.startGeo };
    switch (session.mode) {
      case "move":
        next.x = session.startGeo.x + dx;
        next.y = session.startGeo.y + dy;
        break;
      case "resize-se":
        next.width = Math.max(minWidth, session.startGeo.width + dx);
        next.height = Math.max(minHeight, session.startGeo.height + dy);
        break;
      case "resize-sw":
        next.width = Math.max(minWidth, session.startGeo.width - dx);
        next.x = session.startGeo.x + (session.startGeo.width - next.width);
        next.height = Math.max(minHeight, session.startGeo.height + dy);
        break;
      case "resize-ne":
        next.width = Math.max(minWidth, session.startGeo.width + dx);
        next.height = Math.max(minHeight, session.startGeo.height - dy);
        next.y = session.startGeo.y + (session.startGeo.height - next.height);
        break;
      case "resize-nw":
        next.width = Math.max(minWidth, session.startGeo.width - dx);
        next.x = session.startGeo.x + (session.startGeo.width - next.width);
        next.height = Math.max(minHeight, session.startGeo.height - dy);
        next.y = session.startGeo.y + (session.startGeo.height - next.height);
        break;
      default:
        return;
    }
    setDraftGeo(next);
  }, [minHeight, minWidth]);

  const onPointerUp = useCallback(() => {
    const session = dragRef.current;
    dragRef.current = null;
    setDraftGeo((current) => {
      if (current && session) {
        onGeometryChange(current);
      }
      return null;
    });
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
  }, [onGeometryChange, onPointerMove]);

  const startDrag = useCallback((mode: DragMode) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    dragRef.current = {
      mode,
      startX: e.clientX,
      startY: e.clientY,
      startGeo: geometry,
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }, [geometry, onPointerMove, onPointerUp]);

  useEffect(() => {
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [onPointerMove, onPointerUp]);

  // Esc closes only when the panel itself is focused (or focus is inside it).
  // We use containment + stopPropagation so other Esc consumers (modals,
  // command palette, conflicts dialog) keep working when the scratchpad is
  // not the focused subtree.
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      const card = cardRef.current;
      const target = e.target as Node | null;
      if (!card) return;
      if (target && card.contains(target)) {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKey, true);
    return () => window.removeEventListener("keydown", handleKey, true);
  }, [open, onClose]);

  if (!mounted) return null;

  // color-mix lets the floating card track the active theme's `--card` token
  // while preserving the user-controlled opacity slider.
  const bg = `color-mix(in srgb, var(--card) ${opacity * 100}%, transparent)`;

  return (
    // Outer wrapper covers the viewport but is non-interactive — only the
    // card itself receives pointer events. Clicks outside the card pass
    // through to the workspace below.
    <div
      data-floating-panel-id={panelId}
      className="pointer-events-none fixed inset-0"
      style={{ zIndex }}
    >
      <div
        ref={cardRef}
        data-state={visible ? "open" : "closed"}
        className="pointer-events-auto absolute flex flex-col overflow-hidden rounded-lg border border-border shadow-lg transition-[opacity,transform] duration-100 ease-out data-[state=closed]:opacity-0 data-[state=closed]:scale-[0.97] data-[state=open]:opacity-100 data-[state=open]:scale-100"
        style={{
          left: effective.x,
          top: effective.y,
          width: effective.width,
          height: effective.height,
          background: bg,
          transformOrigin: "center",
        }}
        tabIndex={-1}
      >
        <div
          className="flex h-8 shrink-0 items-center justify-between gap-2 px-3 border-b border-border/50 cursor-move select-none"
          onPointerDown={startDrag("move")}
        >
          <div className="flex items-center gap-2 text-[11px] text-foreground/80 truncate">
            {title}
          </div>
          <div className="flex items-center gap-1">
            {toolbar}
            <button
              type="button"
              aria-label="Close"
              onClick={onClose}
              className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary/60 hover:text-foreground transition-colors"
            >
              <X size={12} />
            </button>
          </div>
        </div>

        <div className="flex-1 min-h-0 overflow-hidden">{children}</div>

        {/* Resize handles. Bottom-right is the conventional one; we also
            expose the other three corners for parity. They are 12px hit
            zones, invisibly nested in the card border. */}
        <div
          className="absolute right-0 bottom-0 size-3 cursor-se-resize"
          onPointerDown={startDrag("resize-se")}
        />
        <div
          className="absolute left-0 bottom-0 size-3 cursor-sw-resize"
          onPointerDown={startDrag("resize-sw")}
        />
        <div
          className="absolute right-0 top-0 size-3 cursor-ne-resize"
          onPointerDown={startDrag("resize-ne")}
        />
        <div
          className="absolute left-0 top-0 size-3 cursor-nw-resize"
          onPointerDown={startDrag("resize-nw")}
        />
      </div>
    </div>
  );
}
