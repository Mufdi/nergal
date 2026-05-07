import { useState, useEffect, useRef } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Workspace } from "@/stores/workspace";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaces: Workspace[];
  preselectedId?: string | null;
  onPick: (workspaceId: string) => void;
}

export function ProjectPickerModal({ open, onOpenChange, workspaces, preselectedId, onPick }: Props) {
  const [selectedIdx, setSelectedIdx] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const idx = preselectedId ? workspaces.findIndex((w) => w.id === preselectedId) : 0;
    setSelectedIdx(idx >= 0 ? idx : 0);
    requestAnimationFrame(() => listRef.current?.focus());
  }, [open, preselectedId, workspaces]);

  function handleKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (workspaces.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => (i + 1) % workspaces.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => (i - 1 + workspaces.length) % workspaces.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const ws = workspaces[selectedIdx];
      if (ws) {
        onPick(ws.id);
        onOpenChange(false);
      }
    } else if (/^[1-9]$/.test(e.key)) {
      const i = parseInt(e.key, 10) - 1;
      if (i < workspaces.length) {
        e.preventDefault();
        onPick(workspaces[i].id);
        onOpenChange(false);
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New session — select project</DialogTitle>
        </DialogHeader>
        <div
          ref={listRef}
          tabIndex={0}
          onKeyDown={handleKeyDown}
          className="flex flex-col gap-1 max-h-72 overflow-y-auto outline-none rounded focus:ring-1 focus:ring-orange-500/50"
        >
          {workspaces.map((w, i) => (
            <button
              key={w.id}
              onClick={() => { onPick(w.id); onOpenChange(false); }}
              className={`flex items-center gap-2 rounded-md px-3 py-2 text-left text-[11px] transition-colors ${
                i === selectedIdx
                  ? "border border-orange-500 bg-orange-500/10 text-foreground"
                  : "border border-border bg-card text-foreground/70 hover:bg-secondary hover:text-foreground"
              }`}
            >
              {i < 9 && (
                <span className="flex size-4 shrink-0 items-center justify-center rounded-sm bg-muted/50 text-[9px] font-medium tabular-nums text-muted-foreground">
                  {i + 1}
                </span>
              )}
              <span className="flex-1 truncate">{w.name}</span>
              <span className="text-[9px] text-muted-foreground/60">{w.sessions.length} sessions</span>
            </button>
          ))}
          {workspaces.length === 0 && (
            <p className="text-[11px] text-muted-foreground py-4 text-center">No projects. Add one with Ctrl+Shift+N.</p>
          )}
        </div>
        <p className="text-[10px] text-muted-foreground/60">↑↓ navigate • 1–9 jump • Enter to pick • Esc to cancel</p>
      </DialogContent>
    </Dialog>
  );
}
