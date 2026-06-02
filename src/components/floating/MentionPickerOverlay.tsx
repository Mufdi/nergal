import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { FileText } from "lucide-react";

export interface MentionItem {
  key: string;
  label: string;
  sublabel?: string;
}

interface MentionPickerOverlayProps {
  items: MentionItem[];
  selectedIndex: number;
  position: { left: number; top: number };
  onSelect: (index: number) => void;
  onHover: (index: number) => void;
  hint?: string;
}

export function MentionPickerOverlay({
  items,
  selectedIndex,
  position,
  onSelect,
  onHover,
  hint,
}: MentionPickerOverlayProps) {
  const listRef = useRef<HTMLDivElement>(null);
  // Keyboard nav past the scroll fold must follow the selection into view —
  // arrow keys move selectedIndex but don't scroll the overflow container.
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>("[data-mention-selected]");
    el?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  if (items.length === 0) return null;
  const activeId = `mention-option-${selectedIndex}`;
  // Portal to <body>: the overlay is fixed-positioned with viewport coords from
  // CodeMirror's coordsAtPos, but the FloatingPanel card it renders inside has
  // `transform: scale(1)` (data-[state=open]), which makes the card the
  // containing block for fixed descendants — so the overlay would anchor to the
  // card, not the viewport. <body> has no transform, restoring viewport math.
  return createPortal(
    <div
      ref={listRef}
      role="listbox"
      aria-label="Vault notes"
      aria-activedescendant={activeId}
      // Focus stays in the textarea/editor driving the picker; the listbox is
      // navigated remotely, so it must not steal the tab sequence.
      tabIndex={-1}
      className="cluihud-glow fixed z-[60] flex max-h-60 w-72 flex-col overflow-hidden rounded-lg border-2 border-primary bg-card shadow-lg"
      style={{ left: position.left, top: position.top }}
    >
      {hint && (
        <div className="shrink-0 border-b border-border/40 px-3 py-1 text-[10px] text-muted-foreground/70">
          {hint}
        </div>
      )}
      <div className="min-h-0 overflow-y-auto py-1">
      {items.map((item, idx) => {
        const isSelected = idx === selectedIndex;
        return (
          <button
            key={item.key}
            id={`mention-option-${idx}`}
            type="button"
            role="option"
            aria-selected={isSelected}
            data-mention-selected={isSelected ? "" : undefined}
            // mousedown (not click) so selecting doesn't blur the textarea first.
            onMouseDown={(e) => {
              e.preventDefault();
              onSelect(idx);
            }}
            onMouseEnter={() => onHover(idx)}
            className={`flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors ${
              isSelected ? "bg-secondary text-foreground" : "text-foreground/80 hover:bg-secondary/50"
            }`}
          >
            <FileText className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-xs">{item.label}</span>
              {item.sublabel && (
                <span className="truncate text-[10px] text-muted-foreground">{item.sublabel}</span>
              )}
            </span>
          </button>
        );
      })}
      </div>
    </div>,
    document.body,
  );
}
