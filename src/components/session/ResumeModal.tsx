import { useEffect, useRef, useState } from "react";
import { Play, List } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface ResumeModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionName: string;
  onSelect: (mode: "continue" | "resume_pick") => void;
}

const OPTIONS: Array<{ mode: "continue" | "resume_pick"; icon: typeof Play; title: string; hint: string }> = [
  { mode: "continue", icon: Play, title: "Continue last conversation", hint: "Pick up where you left off" },
  { mode: "resume_pick", icon: List, title: "Choose a different conversation", hint: "Browse previous conversations" },
];

export function ResumeModal({
  open,
  onOpenChange,
  sessionName,
  onSelect,
}: ResumeModalProps) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setSelectedIdx(0);
    requestAnimationFrame(() => listRef.current?.focus());
  }, [open]);

  function handleSelect(mode: "continue" | "resume_pick") {
    onSelect(mode);
    onOpenChange(false);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIdx((i) => (i + 1) % OPTIONS.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIdx((i) => (i - 1 + OPTIONS.length) % OPTIONS.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      handleSelect(OPTIONS[selectedIdx].mode);
    }
  }

  // Window-level capture so base-ui Dialog can't intercept Enter/Escape first.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Enter" && e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const active = document.activeElement as HTMLElement | null;
      if (active && active !== listRef.current && !listRef.current?.contains(active)) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Enter") {
        handleSelect(OPTIONS[selectedIdx].mode);
      } else if (e.key === "ArrowDown") {
        setSelectedIdx((i) => (i + 1) % OPTIONS.length);
      } else if (e.key === "ArrowUp") {
        setSelectedIdx((i) => (i - 1 + OPTIONS.length) % OPTIONS.length);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, selectedIdx]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>Resume "{sessionName}"</DialogTitle>
        </DialogHeader>
        <div
          ref={listRef}
          tabIndex={0}
          onKeyDown={handleKeyDown}
          className="flex flex-col gap-2 outline-none rounded focus:ring-1 focus:ring-orange-500/50"
        >
          {OPTIONS.map((opt, i) => {
            const Icon = opt.icon;
            return (
              <button
                key={opt.mode}
                onClick={() => handleSelect(opt.mode)}
                className={`flex items-center gap-3 rounded-lg border p-3 text-left transition-colors ${
                  i === selectedIdx
                    ? "border-orange-500 bg-orange-500/10 text-foreground"
                    : "border-border/50 bg-card hover:bg-secondary/60"
                }`}
              >
                <Icon className="size-4 shrink-0 text-foreground/70" />
                <div>
                  <div className="text-[12px] font-medium text-foreground">{opt.title}</div>
                  <div className="text-[10px] text-muted-foreground">{opt.hint}</div>
                </div>
              </button>
            );
          })}
        </div>
        <p className="text-[10px] text-muted-foreground/60">↑↓ navigate • Enter to select • Esc to cancel</p>
      </DialogContent>
    </Dialog>
  );
}
