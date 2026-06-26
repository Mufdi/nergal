import { useEffect, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { invoke } from "@/lib/tauri";
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
import { toastsAtom } from "@/stores/toast";
import { linearIssuesAtom, linearSendConfirmAtom } from "@/stores/linear";

/// Send-as-prompt confirmation (mirrors ClickUpSendConfirmDialog): the send
/// auto-submits the composed brief as a turn, so the user reviews exactly WHAT
/// will be submitted before confirming. A send while the agent is mid-turn
/// rides the agent's own prompt queueing (CC queues natively).
export function LinearSendConfirmDialog() {
  const [request, setRequest] = useAtom(linearSendConfirmAtom);
  const issues = useAtomValue(linearIssuesAtom);
  const addToast = useSetAtom(toastsAtom);
  const [compose, setCompose] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  useEffect(() => {
    setCompose(null);
    setError(null);
    setSending(false);
    if (!request) return;
    let cancelled = false;
    invoke<string>("linear_compose_issue_prompt", { issueId: request.issueId })
      .then((c) => {
        if (!cancelled) setCompose(c);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [request]);

  const issue = request ? issues.find((i) => i.id === request.issueId) : undefined;
  const issueName = request ? (issue?.identifier ?? issue?.title ?? request.issueId) : "";

  async function handleConfirm() {
    if (!request || sending) return;
    setSending(true);
    try {
      // Re-composes fresh on the backend before pasting + submitting.
      await invoke("linear_send_issue_as_prompt", {
        sessionId: request.sessionId,
        issueId: request.issueId,
      });
      addToast({ message: "Issue sent as prompt", description: issueName, type: "success" });
      setRequest(null);
    } catch (err) {
      addToast({ message: "Send failed", description: String(err), type: "error" });
      setSending(false);
    }
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && setRequest(null)}>
      <DialogContent
        className="sm:max-w-lg"
        onKeyDown={(e) => {
          // No free-text field here (read-only preview) — Enter confirms.
          // preventDefault + stop so it can't bubble to a hovered sidebar row.
          if (e.key === "Enter" && compose !== null && !sending) {
            e.preventDefault();
            e.stopPropagation();
            void handleConfirm();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle className="truncate">{issueName}</DialogTitle>
          <DialogDescription>
            This issue brief will be submitted as a turn to the active session — review it
            before sending.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-col gap-2">
          {error ? (
            <p className="text-[11px] text-red-400">{error}</p>
          ) : compose !== null ? (
            <pre className="max-h-[40vh] overflow-y-auto whitespace-pre-wrap rounded border border-border bg-secondary/30 p-2 font-mono text-[10px] leading-4 text-foreground/80">
              {compose}
            </pre>
          ) : (
            <div className="flex flex-col gap-2 py-4">
              <span className="text-xs text-muted-foreground">Composing…</span>
              <ProgressBar className="max-w-40" />
            </div>
          )}
        </div>

        <DialogFooter className="flex-nowrap gap-1.5">
          <Button variant="secondary" size="sm" onClick={() => setRequest(null)}>
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button size="sm" onClick={() => void handleConfirm()} disabled={compose === null || sending}>
            {sending ? "Sending…" : "Send as prompt"}
            <Kbd keys="enter" tone="onPrimary" className="ml-1.5" />
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
