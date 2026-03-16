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

export function ResumeModal({
  open,
  onOpenChange,
  sessionName,
  onSelect,
}: ResumeModalProps) {
  function handleSelect(mode: "continue" | "resume_pick") {
    onSelect(mode);
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>Resume "{sessionName}"</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          <button
            onClick={() => handleSelect("continue")}
            className="flex items-center gap-3 rounded-lg border border-border/50 bg-card p-3 text-left transition-colors hover:bg-secondary/60"
          >
            <Play className="size-4 shrink-0 text-foreground/70" />
            <div>
              <div className="text-[12px] font-medium text-foreground">Continue last conversation</div>
              <div className="text-[10px] text-muted-foreground">Pick up where you left off</div>
            </div>
          </button>
          <button
            onClick={() => handleSelect("resume_pick")}
            className="flex items-center gap-3 rounded-lg border border-border/50 bg-card p-3 text-left transition-colors hover:bg-secondary/60"
          >
            <List className="size-4 shrink-0 text-foreground/70" />
            <div>
              <div className="text-[12px] font-medium text-foreground">Choose a different conversation</div>
              <div className="text-[10px] text-muted-foreground">Browse previous conversations</div>
            </div>
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
