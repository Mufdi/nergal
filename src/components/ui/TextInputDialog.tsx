import { useEffect, useRef, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";

interface TextInputDialogProps {
  open: boolean;
  title: string;
  description?: string;
  initialValue?: string;
  placeholder?: string;
  confirmLabel?: string;
  mono?: boolean;
  onSubmit: (value: string) => void;
  onOpenChange: (open: boolean) => void;
}

export function TextInputDialog({
  open,
  title,
  description,
  initialValue = "",
  placeholder,
  confirmLabel = "Save",
  mono = false,
  onSubmit,
  onOpenChange,
}: TextInputDialogProps) {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setValue(initialValue);
    const raf = requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
    return () => cancelAnimationFrame(raf);
  }, [open, initialValue]);

  function submit() {
    const trimmed = value.trim();
    if (!trimmed) return;
    onOpenChange(false);
    onSubmit(trimmed);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {description && (
          <p className="text-[11px] text-muted-foreground">{description}</p>
        )}
        <Input
          ref={inputRef}
          value={value}
          placeholder={placeholder}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
          className={mono ? "font-mono text-[12px]" : "text-[12px]"}
        />
        <DialogFooter className="flex-nowrap gap-1.5">
          <Button size="sm" variant="secondary" onClick={() => onOpenChange(false)}>
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button size="sm" onClick={submit} disabled={!value.trim()}>
            {confirmLabel} <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
