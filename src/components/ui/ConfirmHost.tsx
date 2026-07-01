import { useEffect, useSyncExternalStore } from "react";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import {
  getActiveConfirm,
  resolveConfirm,
  subscribeConfirm,
} from "@/lib/confirm";

/// Single mounted host that renders `confirm()` requests as a Base UI dialog
/// matching the branch-rename mini-modal (iconless, `Kbd` chips in the footer
/// buttons). One instance lives in Workspace; `@/lib/confirm` drives it.
///
/// Keyboard: Esc cancels, Enter confirms — both handled at the dialog level
/// (Enter via the content-level `onKeyDownCapture`, Esc via Base UI). Initial
/// focus goes to the dialog container (not a button): with no button focused,
/// arrow keys can't land a ring on Cancel while the `Kbd` chip says Enter
/// confirms, so Enter is unambiguously the proposed (destructive) action. The
/// `Kbd` chips (esc / enter) are the affordance.
export function ConfirmHost() {
  const active = useSyncExternalStore(
    subscribeConfirm,
    getActiveConfirm,
    getActiveConfirm,
  );

  useEffect(() => {
    if (!active) return;
    const raf = requestAnimationFrame(() => {
      // Focus the popup itself, not a footer button — the last mounted
      // dialog-content is this confirm (portaled last while active is truthy).
      const nodes = document.querySelectorAll<HTMLElement>('[data-slot="dialog-content"]');
      nodes[nodes.length - 1]?.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [active]);

  const opts = active?.opts;
  const destructive = opts?.destructive ?? false;

  return (
    <Dialog
      open={!!active}
      onOpenChange={(open) => {
        if (!open) resolveConfirm(false);
      }}
    >
      {opts && (
        <DialogContent
          showCloseButton={false}
          tabIndex={-1}
          className="max-w-sm"
          onKeyDownCapture={(e) => {
            // Capture phase so Enter confirms even though focus rests on Cancel:
            // we stop the event before it reaches the button, so the button's
            // native Enter-activation (which would cancel) never fires. Without
            // this, keyboard Enter cancels while only a mouse click could confirm.
            if (e.key === "Enter") {
              e.preventDefault();
              e.stopPropagation();
              resolveConfirm(true);
            }
          }}
        >
          <DialogHeader>
            <DialogTitle>{opts.title}</DialogTitle>
          </DialogHeader>
          {opts.body && (
            <div
              className="text-[12px] leading-relaxed text-muted-foreground"
              // Body is HTML by contract; callers escape user-controlled input.
              dangerouslySetInnerHTML={{ __html: opts.body }}
            />
          )}
          <DialogFooter className="flex-nowrap gap-1.5">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => resolveConfirm(false)}
            >
              {opts.cancelLabel ?? "Cancel"}
              <Kbd keys="esc" className="ml-1.5" />
            </Button>
            <Button
              size="sm"
              variant={destructive ? "destructive" : "default"}
              onClick={() => resolveConfirm(true)}
            >
              {opts.confirmLabel ?? "Confirm"}
              <Kbd
                keys="enter"
                tone={destructive ? "subtle" : "onPrimary"}
                className="ml-1.5"
              />
            </Button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
