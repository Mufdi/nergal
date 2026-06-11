import { useEffect, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { Loader2 } from "lucide-react";
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
import { toastsAtom } from "@/stores/toast";
import {
  clickupRebindConfirmAtom,
  clickupSendConfirmAtom,
  clickupTasksAtom,
  performBindTaskAction,
} from "@/stores/clickup";

/// Send-as-prompt confirmation (Decision 6, reframed 2026-06-11): the send
/// auto-submits the composed brief as a turn, so the user reviews exactly
/// WHAT will be submitted before confirming. A send while the agent is
/// mid-turn rides the agent's own prompt queueing (CC queues natively).
export function ClickUpSendConfirmDialog() {
  const [request, setRequest] = useAtom(clickupSendConfirmAtom);
  const tasks = useAtomValue(clickupTasksAtom);
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
    invoke<string>("clickup_compose_task_prompt", { taskId: request.taskId })
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

  const taskName = request
    ? tasks.find((t) => t.id === request.taskId)?.name ?? request.taskId
    : "";

  async function handleConfirm() {
    if (!request || sending) return;
    setSending(true);
    try {
      // Re-composes fresh on the backend before pasting + submitting.
      await invoke("clickup_send_task_as_prompt", {
        sessionId: request.sessionId,
        taskId: request.taskId,
      });
      addToast({ message: "Task sent as prompt", description: taskName, type: "success" });
      setRequest(null);
    } catch (err) {
      addToast({ message: "Send failed", description: String(err), type: "error" });
      setSending(false);
    }
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && setRequest(null)}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="truncate">{taskName}</DialogTitle>
          <DialogDescription>
            This task brief will be submitted as a turn to the active session — review it
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
            <div className="flex items-center gap-2 py-4 text-xs text-muted-foreground">
              <Loader2 size={13} className="animate-spin" /> Composing…
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => setRequest(null)}>
            Cancel
          </Button>
          <Button size="sm" onClick={() => void handleConfirm()} disabled={compose === null || sending}>
            {sending ? "Sending…" : "Send as prompt"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/// Rebind confirmation (Decision 2): a session has one active task; binding
/// over an existing one replaces the write-back target, so it is confirmed.
export function ClickUpRebindConfirmDialog() {
  const [request, setRequest] = useAtom(clickupRebindConfirmAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const performBind = useSetAtom(performBindTaskAction);

  const name = (id: string) => tasks.find((t) => t.id === id)?.name ?? id;

  async function handleReplace() {
    if (!request) return;
    setRequest(null);
    await performBind({ sessionId: request.sessionId, taskId: request.taskId });
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && setRequest(null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Replace active task?</DialogTitle>
          <DialogDescription>
            This session is bound to "{request ? name(request.currentTaskId) : ""}". Binding "
            {request ? name(request.taskId) : ""}" replaces it as the write-back target. The
            replaced task stays in ClickUp.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => setRequest(null)}>
            Cancel
          </Button>
          <Button size="sm" onClick={() => void handleReplace()}>
            Replace
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
