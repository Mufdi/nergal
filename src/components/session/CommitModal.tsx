import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

interface CommitModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (lang: string) => void;
}

export function CommitModal({ open, onOpenChange, onConfirm }: CommitModalProps) {
  const [lang, setLang] = useState("en");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Commit</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <label className="text-[11px] text-muted-foreground">
            Commit message language:
          </label>
          <div className="flex gap-2">
            <button
              onClick={() => setLang("en")}
              className={`flex-1 rounded-md border px-3 py-2 text-[11px] transition-colors ${
                lang === "en"
                  ? "border-orange-500 bg-orange-500/10 text-foreground"
                  : "border-border bg-card text-muted-foreground hover:text-foreground"
              }`}
            >
              English
            </button>
            <button
              onClick={() => setLang("es")}
              className={`flex-1 rounded-md border px-3 py-2 text-[11px] transition-colors ${
                lang === "es"
                  ? "border-orange-500 bg-orange-500/10 text-foreground"
                  : "border-border bg-card text-muted-foreground hover:text-foreground"
              }`}
            >
              Español
            </button>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => {
              onConfirm(lang);
              onOpenChange(false);
            }}
          >
            Commit
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
