import { useEffect, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { AlertTriangle, Loader2 } from "lucide-react";
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
  type ClickUpComposeConfirm,
  type ClickUpSendOutcome,
} from "@/stores/clickup";

/// Send-as-prompt confirmation (Decision 6, reframed 2026-06-11): the
/// immediate path auto-submits the composed brief as a turn, so the user
/// reviews exactly WHAT will be submitted + the guard state before confirming.
export function ClickUpSendConfirmDialog() {
  const [request, setRequest] = useAtom(clickupSendConfirmAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const addToast = useSetAtom(toastsAtom);
  const [compose, setCompose] = useState<ClickUpComposeConfirm | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [guardEnabled, setGuardEnabled] = useState(false);
  const [enabling, setEnabling] = useState(false);

  useEffect(() => {
    setCompose(null);
    setError(null);
    setSending(false);
    setGuardEnabled(false);
    setEnabling(false);
    if (!request) return;
    let cancelled = false;
    invoke<ClickUpComposeConfirm>("clickup_compose_task_prompt", {
      sessionId: request.sessionId,
      taskId: request.taskId,
    })
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

  async function handleConfirm() {
    if (!request || sending) return;
    setSending(true);
    try {
      // Re-composes fresh on the backend; outcome toasts + the pending-send
      // indicator ride the clickup:send-* events.
      await invoke<ClickUpSendOutcome>("clickup_send_task_as_prompt", {
        sessionId: request.sessionId,
        taskId: request.taskId,
      });
      setRequest(null);
    } catch (err) {
      addToast({ message: "Send failed", description: String(err), type: "error" });
      setSending(false);
    }
  }

  const taskName = request
    ? tasks.find((t) => t.id === request.taskId)?.name ?? request.taskId
    : "";

  async function handleEnableGuard() {
    if (enabling) return;
    setEnabling(true);
    try {
      await invoke<string>("setup_hooks");
      setGuardEnabled(true);
    } catch (err) {
      addToast({ message: "Enable guard failed", description: String(err), type: "error" });
    } finally {
      setEnabling(false);
    }
  }

  return (
    <Dialog open={request !== null} onOpenChange={(open) => !open && setRequest(null)}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>ClickUp task brief — review before sending</DialogTitle>
          <DialogDescription>
            "{taskName}" will be submitted as a turn to the active session.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-col gap-2">
          {error ? (
            <p className="text-[11px] text-red-400">{error}</p>
          ) : compose ? (
            <pre className="max-h-[40vh] overflow-y-auto whitespace-pre-wrap rounded border border-border bg-secondary/30 p-2 font-mono text-[10px] leading-4 text-foreground/80">
              {compose.markdown}
            </pre>
          ) : (
            <div className="flex items-center gap-2 py-4 text-xs text-muted-foreground">
              <Loader2 size={13} className="animate-spin" /> Composing…
            </div>
          )}
          {compose &&
            (compose.guard_active ? (
              <p className="text-[10px] text-muted-foreground">Turn-safety guard active.</p>
            ) : compose.guard_hint === "restart_session" ? (
              <p className="flex items-start gap-1.5 text-[10px] text-yellow-500">
                <AlertTriangle size={11} className="mt-px shrink-0" />
                <span>
                  Turn-safety guard installed but not active in this session yet — restart
                  the session (close + reopen the tab) to activate.
                </span>
              </p>
            ) : guardEnabled ? (
              <p className="text-[10px] text-muted-foreground">
                Guard enabled — restart this session (close + reopen) to activate it here.
              </p>
            ) : (
              <div className="flex items-start gap-1.5 text-[10px] text-yellow-500">
                <AlertTriangle size={11} className="mt-px shrink-0" />
                <span className="flex-1">
                  Turn-safety guard off: if the agent is mid-response, this prompt could
                  interleave with it. Enable it once and it covers all future sessions.
                </span>
                <Button
                  variant="outline"
                  size="xs"
                  className="shrink-0"
                  disabled={enabling}
                  onClick={() => void handleEnableGuard()}
                >
                  {enabling ? "Enabling…" : "Enable guard"}
                </Button>
              </div>
            ))}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => setRequest(null)}>
            Cancel
          </Button>
          <Button size="sm" onClick={() => void handleConfirm()} disabled={!compose || sending}>
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
