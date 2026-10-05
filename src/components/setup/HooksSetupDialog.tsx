import { useEffect, useRef, useState } from "react";
import { useSetAtom } from "jotai";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { invoke } from "@/lib/tauri";
import { canConfigure, shouldOpenHooksDialog } from "@/lib/hooksSetup";
import type { HooksStatus } from "@/lib/types";
import { toastsAtom } from "@/stores/toast";

export function HooksSetupDialog() {
  const setToasts = useSetAtom(toastsAtom);
  const [status, setStatus] = useState<HooksStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<HooksStatus>("setup_hooks_status")
      .then((result) => {
        if (cancelled) return;
        setStatus(result);
        setOpen(shouldOpenHooksDialog(result));
      })
      .catch((err) => {
        console.warn("setup_hooks_status failed", err);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const raf = requestAnimationFrame(() => contentRef.current?.focus());
    return () => cancelAnimationFrame(raf);
  }, [open]);

  function handleSkip() {
    if (busy) return;
    setOpen(false);
  }

  async function handleConfigure() {
    setBusy(true);
    setError(null);
    try {
      await invoke("setup_hooks");
      setOpen(false);
      setToasts({
        message: "Hooks configured",
        description: "New Claude Code sessions will report to Nergal.",
        type: "success",
      });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  if (!status) return null;

  const configurable = canConfigure(status);

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleSkip()}>
      <DialogContent
        ref={contentRef}
        showCloseButton={false}
        tabIndex={-1}
        className="sm:max-w-xl overflow-hidden"
        onKeyDownCapture={(e) => {
          if (e.target instanceof HTMLButtonElement) return;
          if (e.key === "Enter") {
            e.preventDefault();
            e.stopPropagation();
            if (configurable && !busy) void handleConfigure();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>Connect Nergal to Claude Code</DialogTitle>
          <DialogDescription className="break-words">
            Activities, Tasks, plan review and questions need Nergal's hooks in Claude Code's
            settings. They are not registered yet.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-3 py-2">
          <p className="text-xs text-muted-foreground break-words">
            Hooks {configurable ? "will" : "would"} be added to <code className="text-foreground">{status.settings_path}</code>.
            A backup is saved as <code className="text-foreground">settings.json.bak</code>.
          </p>
          {!configurable && (
            <p className="text-xs text-foreground break-words">
              <code>nergal</code> is not on your PATH (typical for AppImage installs), so the hooks
              could not run. Install the .deb or add <code>nergal</code> to your PATH, then
              relaunch.
            </p>
          )}
          {error && <p role="alert" className="text-xs text-destructive break-words">{error}</p>}
        </div>

        <DialogFooter className="flex-nowrap gap-1.5">
          <Button
            variant="secondary"
            className="focus-visible:ring-0 focus-visible:border-transparent"
            onClick={handleSkip}
            disabled={busy}
          >
            Skip <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button
            className="focus-visible:ring-0 focus-visible:border-transparent"
            onClick={() => void handleConfigure()}
            disabled={busy || !configurable}
          >
            {busy ? "Configuring…" : "Configure"}
            {configurable && <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
