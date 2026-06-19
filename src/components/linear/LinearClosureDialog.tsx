import { useEffect, useRef, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import { PulseDots } from "@/components/ui/PulseDots";
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
import { Kbd } from "@/components/ui/kbd";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { toastsAtom } from "@/stores/toast";
import {
  linearBindingMapAtom,
  linearClosedOutAtom,
  linearClosureOfferAtom,
  linearDetailIssueIdAtom,
  linearIssuesAtom,
  type WorkflowStateView,
} from "@/stores/linear";

/// Serde shape from Rust: #[serde(tag = "status", rename_all = "snake_case")]
function parseStateOutcome(raw: unknown): "skipped" | "ok" | { failed: string } {
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

/// Neutralize Linear @mention syntax in the PR link prefill so it cannot ping
/// the team. Zero-width space mirrors the Rust sanitize_comment_text logic.
function sanitizeForDisplay(text: string): string {
  return text.replace(/(@)(\w)/g, "$1​$2");
}

export function LinearClosureDialog() {
  const [offer, setOffer] = useAtom(linearClosureOfferAtom);
  const issues = useAtomValue(linearIssuesAtom);
  const addToast = useSetAtom(toastsAtom);
  const setBindingMap = useSetAtom(linearBindingMapAtom);
  const setClosedOut = useSetAtom(linearClosedOutAtom);
  const setDetailIssueId = useSetAtom(linearDetailIssueIdAtom);

  const [states, setStates] = useState<WorkflowStateView[]>([]);
  const [statesLoading, setStatesLoading] = useState(false);
  const [selectedStateId, setSelectedStateId] = useState<string>("");
  // Index cursor over [state chips…, comment row]
  const [cursor, setCursor] = useState(0);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const commentRef = useRef<HTMLTextAreaElement | null>(null);
  const [comment, setComment] = useState<string>("");
  const [executing, setExecuting] = useState(false);
  // After an uncertain comment outcome, offer a verify+retry path.
  const [uncertainComment, setUncertainComment] = useState<{ text: string; sentAtSecs: number } | null>(null);
  const [retrying, setRetrying] = useState(false);

  const issue = offer ? (issues.find((i) => i.id === offer.issueId) ?? null) : null;
  const issueName = issue?.identifier ?? issue?.title ?? offer?.issueId ?? "";
  const teamId = issue?.teamId ?? "";

  const hasState = selectedStateId.length > 0;
  const hasComment = comment.trim().length > 0;
  // Confirm is always armed — neither selected still closes out locally.
  const canConfirm = !executing;

  // Fetch team states when offer opens and we know the team.
  useEffect(() => {
    setSelectedStateId("");
    setUncertainComment(null);
    if (!offer || !teamId) {
      setStates([]);
      return;
    }
    let cancelled = false;
    setStatesLoading(true);
    invoke<WorkflowStateView[]>("linear_read_team_states", { teamId })
      .then((s) => { if (!cancelled) setStates(s); })
      .catch(() => { if (!cancelled) setStates([]); })
      .finally(() => { if (!cancelled) setStatesLoading(false); });
    return () => { cancelled = true; };
  }, [offer?.issueId, teamId]);

  // Prefill comment with sanitized PR URL when offer comes from ship-success.
  useEffect(() => {
    if (!offer) { setComment(""); return; }
    setComment(offer.prUrl ? sanitizeForDisplay(offer.prUrl) : "");
  }, [offer?.issueId, offer?.prUrl]);

  // Grab focus into the dialog body when it opens.
  useEffect(() => {
    if (!offer) return;
    setCursor(0);
    const t = setTimeout(() => bodyRef.current?.focus({ preventScroll: true }), 50);
    return () => clearTimeout(t);
  }, [offer?.issueId]);

  function handleBodyKeyDown(e: React.KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "TEXTAREA") return;
    const total = states.length + 1; // chips + comment row
    if (e.key === "ArrowDown" || e.key === "ArrowRight") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, total - 1));
    } else if (e.key === "ArrowUp" || e.key === "ArrowLeft") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter" || e.key === " ") {
      if (e.ctrlKey) return;
      e.preventDefault();
      if (cursor < states.length) {
        const s = states[cursor];
        if (s) setSelectedStateId((prev) => (prev === s.id ? "" : s.id));
      } else {
        commentRef.current?.focus();
      }
    }
  }

  function close() {
    setOffer(null);
    setExecuting(false);
    setUncertainComment(null);
    setRetrying(false);
  }

  function finishCloseOut(description: string) {
    if (offer) {
      const closedIssueId = offer.issueId;
      const closedSessionId = offer.sessionId;
      // Mark closed-out in the durable store (backend already did it via execute_gated_write).
      setClosedOut((prev) => new Set([...prev, closedIssueId]));
      // Reflect the unbind in the runtime binding map (backend already unbound).
      setBindingMap((prev) => ({ ...prev, [closedSessionId]: null }));
      // Close the detail modal so it doesn't linger on the closed issue.
      setDetailIssueId(null);
    }
    addToast({ message: "Issue closed out", description, type: "success" });
    close();
  }

  async function handleConfirm() {
    if (!offer || !canConfirm) return;
    setExecuting(true);
    try {
      const token = await invoke<string>("linear_request_closure_token", {
        issueId: offer.issueId,
        stateId: hasState ? selectedStateId : null,
        comment: hasComment ? comment.trim() : null,
      });
      const raw = await invoke<Record<string, unknown>>("linear_execute_gated_write", {
        token,
        sessionId: offer.sessionId,
      });
      const stateOut = parseStateOutcome(raw["state"]);
      const commentOut = parseCommentOutcome(raw["comment"]);
      handleOutcome(stateOut, commentOut);
    } catch (err) {
      addToast({ message: "Closure failed", description: String(err), type: "error" });
      setExecuting(false);
    }
  }

  function handleOutcome(
    stateOut: ReturnType<typeof parseStateOutcome>,
    commentOut: ReturnType<typeof parseCommentOutcome>,
  ) {
    const stateOk = stateOut === "ok" || stateOut === "skipped";
    const commentOk = commentOut === "posted" || commentOut === "skipped";

    if (typeof commentOut === "object" && "uncertain" in commentOut) {
      setUncertainComment({ text: comment.trim(), sentAtSecs: Math.floor(Date.now() / 1000) });
      addToast({
        message: "Comment status unclear",
        description: "Network error — the comment may or may not have been posted. Verify before retrying.",
        type: "info",
      });
      setExecuting(false);
      return;
    }

    if (!stateOk && typeof stateOut === "object" && "failed" in stateOut) {
      if (commentOut === "posted") {
        addToast({
          message: "Comment posted; state change failed",
          description: `${stateOut.failed} — retry the state change from the issue detail.`,
          type: "info",
        });
      } else {
        addToast({ message: "State change failed", description: stateOut.failed, type: "error" });
      }
    } else if (!commentOk && typeof commentOut === "object" && "failed" in commentOut) {
      addToast({ message: "Comment failed to post", description: commentOut.failed, type: "error" });
    }

    if (stateOk && commentOk) {
      const parts: string[] = [];
      if (stateOut === "ok") {
        const s = states.find((st) => st.id === selectedStateId);
        if (s) parts.push(`State → ${s.name}`);
      }
      if (commentOut === "posted") parts.push("Comment posted");
      finishCloseOut(parts.join(" · ") || "Marked done");
      return;
    }

    // Partial failure already toasted; close.
    close();
  }

  async function handleVerifyRetry() {
    if (!offer || !uncertainComment) return;
    setRetrying(true);
    try {
      const landed = await invoke<boolean>("linear_verify_comment_landed", {
        issueId: offer.issueId,
        body: uncertainComment.text,
        postedAtSecs: uncertainComment.sentAtSecs,
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
      <DialogContent
        className="sm:max-w-md"
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.ctrlKey && canConfirm) {
            e.preventDefault();
            void handleConfirm();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle className="truncate">Close out: {issueName}</DialogTitle>
          <DialogDescription>
            Unbinds the issue and marks it done in Nergal. Optionally move its Linear state and/or post a comment — both are optional.
          </DialogDescription>
        </DialogHeader>

        <div ref={bodyRef} tabIndex={0} onKeyDown={handleBodyKeyDown} className="flex flex-col gap-4 outline-none">
          {/* State picker — optional */}
          <div className="flex flex-col gap-1.5">
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              State (optional)
            </span>
            {statesLoading ? (
              <div className="flex min-h-[1.5rem] flex-col justify-center gap-1">
                <span className="text-[10px] text-muted-foreground">Loading states…</span>
                <ProgressBar />
              </div>
            ) : states.length === 0 ? (
              <p className="min-h-[1.5rem] text-xs text-muted-foreground">No states available for this team.</p>
            ) : (
              <div className="flex min-h-[1.5rem] flex-wrap gap-1.5">
                {states.map((s, i) => (
                  <button
                    key={s.id}
                    type="button"
                    onClick={() => { setCursor(i); setSelectedStateId((prev) => (prev === s.id ? "" : s.id)); }}
                    className={`flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] outline-none transition-colors ${
                      selectedStateId === s.id
                        ? "border border-orange-500 bg-orange-500/10 text-foreground"
                        : "border border-border bg-secondary/40 text-muted-foreground hover:text-foreground"
                    } ${cursor === i ? "ring-1 ring-foreground/50" : ""}`}
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
            {selectedStateId && (() => {
              const s = states.find((st) => st.id === selectedStateId);
              return s ? (
                <p className="text-[10px] text-muted-foreground">
                  Will move issue to{" "}
                  <span className="font-medium text-foreground">{s.name}</span>
                </p>
              ) : null;
            })()}
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
              ref={commentRef}
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              onFocus={() => setCursor(states.length)}
              placeholder="Leave empty to skip the comment"
              className={`h-24 resize-none text-[12px] leading-relaxed ${cursor === states.length ? "ring-1 ring-foreground/50" : ""}`}
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
                  <>Checking <PulseDots className="ml-1.5" /></>
                ) : (
                  "Check if it landed"
                )}
              </Button>
            </div>
          )}
        </div>

        <DialogFooter className="shrink-0 flex-nowrap gap-1.5">
          <Button variant="secondary" size="sm" onClick={close} disabled={executing}>
            Cancel <Kbd keys="esc" className="ml-1.5" />
          </Button>
          <Button
            size="sm"
            onClick={() => void handleConfirm()}
            disabled={!canConfirm}
          >
            {executing ? (
              <>Applying <PulseDots className="ml-1.5" /></>
            ) : (
              <>Close out <Kbd keys="ctrl+enter" tone="onPrimary" className="ml-1.5" /></>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
