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
import { Textarea } from "@/components/ui/textarea";
import { toastsAtom } from "@/stores/toast";
import {
  clickupClosureOfferAtom,
  clickupTasksAtom,
  type ClickUpListStatus,
} from "@/stores/clickup";

/// Serde from Rust: { status: "failed", error: "..." } when discriminated via
/// #[serde(tag = "status", rename_all = "snake_case")]. The tag sits at the
/// top level of the outer object, not inside a nested key.
/// Actual shape emitted: { "status": "failed", "error": "..." }
function parseStatusOutcome(raw: unknown): "skipped" | "ok" | { failed: string } {
  if (typeof raw !== "object" || raw === null) return "skipped";
  const obj = raw as Record<string, unknown>;
  if (obj["status"] === "skipped") return "skipped";
  if (obj["status"] === "ok") return "ok";
  if (obj["status"] === "failed") return { failed: String(obj["error"] ?? "unknown error") };
  return "skipped";
}

function parseCommentOutcome(raw: unknown): "skipped" | "posted" | { failed: string } | { uncertain: string } {
  if (typeof raw !== "object" || raw === null) return "skipped";
  const obj = raw as Record<string, unknown>;
  if (obj["status"] === "skipped") return "skipped";
  if (obj["status"] === "posted") return "posted";
  if (obj["status"] === "failed") return { failed: String(obj["error"] ?? "unknown error") };
  if (obj["status"] === "uncertain") return { uncertain: String(obj["error"] ?? "network timeout") };
  return "skipped";
}

/// Prefill sanitization: PR URL from the ship flow is user-authored but may
/// include agent-generated content in the title portion — strip ClickUp
/// mention (@word) and task-reference (#word) syntax so it cannot ping the
/// team. Zero-width space mirrors the Rust sanitize_comment_text logic.
function sanitizeForDisplay(text: string): string {
  return text.replace(/(@|#)(\w)/g, "$1​$2");
}

export function ClickUpClosureDialog() {
  const [offer, setOffer] = useAtom(clickupClosureOfferAtom);
  const tasks = useAtomValue(clickupTasksAtom);
  const addToast = useSetAtom(toastsAtom);

  const [statuses, setStatuses] = useState<ClickUpListStatus[]>([]);
  const [selectedStatus, setSelectedStatus] = useState<string>("");
  const [comment, setComment] = useState<string>("");
  const [executing, setExecuting] = useState(false);
  // After an uncertain comment outcome, offer a verify+retry path.
  const [uncertainComment, setUncertainComment] = useState<{ text: string; sentAtMs: number } | null>(null);
  const [retrying, setRetrying] = useState(false);

  const task = offer ? tasks.find((t) => t.id === offer.taskId) ?? null : null;
  const taskName = task?.name ?? offer?.taskId ?? "";
  const listId = task?.list_id ?? "";

  const hasStatus = selectedStatus.length > 0;
  const hasComment = comment.trim().length > 0;
  const canConfirm = (hasStatus || hasComment) && !executing;

  // Fetch statuses when the offer opens and we know the list.
  useEffect(() => {
    setSelectedStatus("");
    setUncertainComment(null);
    if (!offer || !listId) {
      setStatuses([]);
      return;
    }
    let cancelled = false;
    invoke<ClickUpListStatus[]>("clickup_read_list_statuses", { listId })
      .then((s) => { if (!cancelled) setStatuses(s); })
      .catch(() => { if (!cancelled) setStatuses([]); });
    return () => { cancelled = true; };
  }, [offer?.taskId, listId]);

  // Prefill comment with sanitized PR URL when the offer comes from ship-success.
  useEffect(() => {
    if (!offer) { setComment(""); return; }
    setComment(offer.prUrl ? sanitizeForDisplay(offer.prUrl) : "");
  }, [offer?.taskId, offer?.prUrl]);

  function close() {
    setOffer(null);
    setExecuting(false);
    setUncertainComment(null);
    setRetrying(false);
  }

  async function handleConfirm() {
    if (!offer || !canConfirm) return;
    setExecuting(true);
    try {
      const token = await invoke<string>("clickup_request_closure_token", {
        taskId: offer.taskId,
        status: hasStatus ? selectedStatus : null,
        comment: hasComment ? comment.trim() : null,
      });
      const raw = await invoke<Record<string, unknown>>("clickup_execute_closure", { token });
      const statusOut = parseStatusOutcome(raw["status"]);
      const commentOut = parseCommentOutcome(raw["comment"]);
      handleOutcome(statusOut, commentOut);
    } catch (err) {
      addToast({ message: "Closure failed", description: String(err), type: "error" });
      setExecuting(false);
    }
  }

  function handleOutcome(
    statusOut: ReturnType<typeof parseStatusOutcome>,
    commentOut: ReturnType<typeof parseCommentOutcome>,
  ) {
    const statusOk = statusOut === "ok" || statusOut === "skipped";
    const commentOk = commentOut === "posted" || commentOut === "skipped";

    if (typeof commentOut === "object" && "uncertain" in commentOut) {
      setUncertainComment({ text: comment.trim(), sentAtMs: Date.now() });
      addToast({
        message: "Comment status unclear",
        description: "Network error — the comment may or may not have been posted. Verify before retrying.",
        type: "info",
      });
      setExecuting(false);
      return;
    }

    if (!statusOk && typeof statusOut === "object" && "failed" in statusOut) {
      if (commentOut === "posted") {
        addToast({
          message: "Comment posted; status change failed",
          description: `${statusOut.failed} — retry the status change from the task detail.`,
          type: "info",
        });
      } else {
        addToast({ message: "Status change failed", description: statusOut.failed, type: "error" });
      }
    } else if (!commentOk && typeof commentOut === "object" && "failed" in commentOut) {
      addToast({ message: "Comment failed to post", description: commentOut.failed, type: "error" });
    } else if (statusOk && commentOk) {
      const parts: string[] = [];
      if (statusOut === "ok") parts.push(`Status → ${selectedStatus}`);
      if (commentOut === "posted") parts.push("Comment posted");
      addToast({
        message: "Task closed out",
        description: parts.join(" · ") || "No changes",
        type: "success",
      });
    }

    // Mirror refresh: the poller will reconcile on the next cycle, but
    // trigger an early read so the task detail's status chip updates sooner.
    void invoke("clickup_read_tasks", {}).catch(() => {});
    close();
  }

  async function handleVerifyRetry() {
    if (!offer || !uncertainComment) return;
    setRetrying(true);
    try {
      const landed = await invoke<boolean>("clickup_verify_comment_landed", {
        taskId: offer.taskId,
        text: uncertainComment.text,
        postedAtMs: uncertainComment.sentAtMs,
      });
      if (landed) {
        addToast({
          message: "Comment confirmed landed",
          description: "The comment was successfully posted.",
          type: "success",
        });
        setUncertainComment(null);
        close();
      } else {
        addToast({
          message: "Comment not found",
          description: "Safe to retry posting.",
          type: "info",
        });
        setUncertainComment(null);
        setExecuting(false);
      }
    } catch (err) {
      addToast({ message: "Verify failed", description: String(err), type: "error" });
      setRetrying(false);
    }
  }

  return (
    <Dialog open={offer !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="truncate">Close out: {taskName}</DialogTitle>
          <DialogDescription>
            Choose what to update in ClickUp. Both halves are optional — confirm only what you want to apply.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          {/* Status picker — optional */}
          <div className="flex flex-col gap-1.5">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Status (optional)
            </span>
            {statuses.length === 0 ? (
              <p className="text-xs text-muted-foreground">No statuses available for this list.</p>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {statuses.map((s) => (
                  <button
                    key={s.name}
                    type="button"
                    onClick={() => setSelectedStatus((prev) => (prev === s.name ? "" : s.name))}
                    className={`flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] transition-colors ${
                      selectedStatus === s.name
                        ? "border border-orange-500 bg-orange-500/10 text-foreground"
                        : "border border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    {s.color && (
                      <span
                        className="size-1.5 shrink-0 rounded-full"
                        style={{ background: s.color }}
                      />
                    )}
                    {s.name}
                  </button>
                ))}
              </div>
            )}
            {selectedStatus && (
              <p className="text-[10px] text-muted-foreground">
                Will move task to{" "}
                <span className="font-medium text-foreground">{selectedStatus}</span>
              </p>
            )}
          </div>

          {/* Comment composer — optional */}
          <div className="flex flex-col gap-1.5">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Comment (optional)
              {offer?.prUrl && (
                <span className="ml-2 normal-case font-normal text-muted-foreground/60">
                  · prefilled with PR link, editable
                </span>
              )}
            </span>
            <Textarea
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              placeholder="Leave empty to skip the comment"
              className="h-24 resize-none text-[12px] leading-relaxed"
              disabled={executing}
            />
          </div>

          {/* Uncertain comment recovery path */}
          {uncertainComment && (
            <div className="rounded border border-yellow-500/40 bg-yellow-500/10 px-3 py-2 text-[11px] text-yellow-300">
              <p className="font-medium">Comment status unclear</p>
              <p className="mt-0.5 text-yellow-300/80">
                The network request timed out — the comment may or may not have been posted.
                Verify before retrying to avoid duplicates.
              </p>
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => void handleVerifyRetry()}
                disabled={retrying}
              >
                {retrying ? (
                  <><Loader2 size={12} className="mr-1.5 animate-spin" />Checking…</>
                ) : (
                  "Check if it landed"
                )}
              </Button>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={close} disabled={executing}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => void handleConfirm()}
            disabled={!canConfirm}
          >
            {executing ? (
              <><Loader2 size={12} className="mr-1.5 animate-spin" />Applying…</>
            ) : (
              "Confirm"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
