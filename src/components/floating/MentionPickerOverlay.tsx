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
}

export function MentionPickerOverlay({
  items,
  selectedIndex,
  position,
  onSelect,
  onHover,
}: MentionPickerOverlayProps) {
  if (items.length === 0) return null;
  return (
    <div
      className="cluihud-glow fixed z-[60] max-h-60 w-72 overflow-y-auto rounded-lg border-2 border-primary bg-card py-1 shadow-lg"
      style={{ left: position.left, top: position.top }}
    >
      {items.map((item, idx) => {
        const isSelected = idx === selectedIndex;
        return (
          <button
            key={item.key}
            type="button"
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
  );
}
