import { useState, useEffect, useRef } from "react";
import { useAtomValue, useSetAtom, useAtom } from "jotai";
import { planDocumentsAtom, defaultPlanState, activePlanReviewStatusAtom, planReviewStatusMapAtom } from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { activeAnnotationsAtom, clearAnnotationsAtom, addAnnotationAtom, serializeAnnotations, annotationModeAtom, annotationScopeAtom, canEnterAnnotationModeAtom } from "@/stores/annotations";
import { hasCapabilityAtom } from "@/stores/agent";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { confirm } from "@/lib/confirm";
import { AnnotatableMarkdownView } from "./AnnotatableMarkdownView";
import { useObsidianMentionPicker } from "@/hooks/useObsidianMentionPicker";
import { appStore } from "@/stores/jotaiStore";
import { focusZoneAtom } from "@/stores/shortcuts";
import { settingsOpenAtom } from "@/stores/config";
import { MessageSquare, Trash2, Highlighter } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from "@/components/ui/tooltip";

interface PlanPanelProps {
  path: string;
}

export function PlanPanel({ path }: PlanPanelProps) {
  const supportsPlanReview = useAtomValue(hasCapabilityAtom("PLAN_REVIEW"));
  const docs = useAtomValue(planDocumentsAtom);
  const plan = path ? (docs[path] ?? defaultPlanState) : defaultPlanState;
  const addToast = useSetAtom(toastsAtom);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const clearAnnotations = useSetAtom(clearAnnotationsAtom);
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const reviewStatus = useAtomValue(activePlanReviewStatusAtom);
  const setReviewStatusMap = useSetAtom(planReviewStatusMapAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const canAnnotate = reviewStatus === "pending_review";
  const isSubmitted = reviewStatus === "submitted";
  const [annotationMode, setAnnotationMode] = useAtom(annotationModeAtom);
  const setScope = useSetAtom(annotationScopeAtom);
  const canEnterAnnotationMode = useAtomValue(canEnterAnnotationModeAtom);
  const setSettingsOpen = useSetAtom(settingsOpenAtom);

  // Claim plan scope while mounted; release on unmount so other panels' scope
  // (null → plan fallback) isn't confused by a stale spec scope.
  useEffect(() => {
    if (!sessionId) return;
    setScope({ kind: "plan", sessionId });
    return () => setScope(null);
  }, [sessionId, setScope]);

  // Auto-exit annotation mode when review status changes away from pending_review
  useEffect(() => {
    if (!canEnterAnnotationMode && annotationMode) setAnnotationMode(false);
  }, [canEnterAnnotationMode, annotationMode, setAnnotationMode]);

  const [showGlobalInput, setShowGlobalInput] = useState(false);
  const [globalComment, setGlobalComment] = useState("");
  const globalInputRef = useRef<HTMLTextAreaElement>(null);
  // The global-comment textarea only mounts while annotating; pass that
  // condition so the picker's listener re-attaches when it appears.
  const mentionOverlay = useObsidianMentionPicker(globalInputRef, showGlobalInput && canAnnotate);

  if (!supportsPlanReview) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center">
        <p className="text-xs text-muted-foreground">
          The active agent does not support plan review.
        </p>
      </div>
    );
  }

  const hasPlan = plan.content.length > 0;
  const backendSessionId = plan.claudeSessionId;
  const decisionPath = plan.decisionPath;

  useEffect(() => {
    if (showGlobalInput) globalInputRef.current?.focus();
  }, [showGlobalInput]);

  useEffect(() => {
    function handleToggleGlobal() { setShowGlobalInput((prev) => !prev); }
    document.addEventListener("nergal:toggle-global-comment", handleToggleGlobal);
    return () => document.removeEventListener("nergal:toggle-global-comment", handleToggleGlobal);
  }, []);

  useEffect(() => {
    function handleToggleAnnotationMode() {
      if (!canEnterAnnotationMode) return;
      setAnnotationMode((prev) => !prev);
    }
    document.addEventListener("nergal:toggle-annotation-mode", handleToggleAnnotationMode);
    return () => document.removeEventListener("nergal:toggle-annotation-mode", handleToggleAnnotationMode);
  }, [canEnterAnnotationMode, setAnnotationMode]);

  function handleRevise() {
    const feedback = serializeAnnotations(annotations, plan.path ?? path);
    if (!backendSessionId || !feedback || !decisionPath) return;
    invoke("submit_plan_decision", {
      sessionId: backendSessionId,
      decisionPath,
      approved: false,
      feedback,
    })
      .then(() => {
        if (sessionId) setReviewStatusMap((prev) => ({ ...prev, [sessionId]: "submitted" }));
        clearAnnotations();
        addToast({ message: "Annotations sent", description: "Claude will revise the plan", type: "success" });
      })
      .catch((err: unknown) => addToast({ message: "Revise Failed", description: String(err), type: "error" }));
  }

  function handleApprove() {
    if (!backendSessionId || !decisionPath) {
      clearAnnotations();
      if (sessionId) setReviewStatusMap((prev) => ({ ...prev, [sessionId]: "idle" }));
      return;
    }
    invoke("submit_plan_decision", { sessionId: backendSessionId, decisionPath, approved: true })
      .then(() => {
        clearAnnotations();
        if (sessionId) setReviewStatusMap((prev) => ({ ...prev, [sessionId]: "idle" }));
      })
      .catch((err: unknown) => addToast({ message: "Approve Failed", description: String(err), type: "error" }));
  }

  function handleGlobalComment() {
    if (showGlobalInput) {
      if (globalComment.trim()) {
        addAnnotation({
          type: "comment",
          target: "[global]",
          content: globalComment.trim(),
          startMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
          endMeta: { parentTagName: "", parentIndex: 0, textOffset: 0 },
        });
        setGlobalComment("");
      }
      setShowGlobalInput(false);
    } else {
      setShowGlobalInput(true);
    }
  }

  // Plan review actions via custom events (dispatched from shortcuts registry)
  useEffect(() => {
    if (!canAnnotate) return;

    function onClear() {
      clearAnnotations();
      addToast({ message: "Cleared", description: "All annotations removed", type: "info" });
    }
    function onApprove() { handleApprove(); }
    function onRevise() { if (annotations.length > 0) handleRevise(); }

    document.addEventListener("nergal:clear-annotations", onClear);
    document.addEventListener("nergal:approve-plan", onApprove);
    document.addEventListener("nergal:revise-plan", onRevise);
    return () => {
      document.removeEventListener("nergal:clear-annotations", onClear);
      document.removeEventListener("nergal:approve-plan", onApprove);
      document.removeEventListener("nergal:revise-plan", onRevise);
    };
  }, [canAnnotate, annotations.length, backendSessionId, decisionPath]);

  // Bare-letter verbs scoped to the plan surface (patterns.md §1/§8): with the
  // panel zone focused during review, A/R/C/X act without a modifier — they
  // bridge to the same custom events as the header buttons. Entering annotation
  // mode keeps its modifier (Ctrl+Shift+H); it's triggered from outside the
  // engaged state. X is confirm-gated because a bare letter is easy to hit by
  // accident.
  useEffect(() => {
    if (!canAnnotate) return;
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (!["KeyA", "KeyR", "KeyC", "KeyX"].includes(e.code)) return;
      const t = e.target as HTMLElement | null;
      if (
        t?.tagName === "INPUT" || t?.tagName === "TEXTAREA"
        || !!t?.closest(".cm-editor") || t?.getAttribute("contenteditable") === "true"
      ) return;
      // Gate on the active focus zone, not the keydown target's DOM ancestry:
      // entering annotation mode orphans activeElement to <body> (the highlight
      // surface isn't focusable), so a `closest()` check wrongly bails and the
      // bare-letter verbs die. The panel's onMouseDown keeps focusZoneAtom in
      // sync on every click inside it, so it stays the canonical signal.
      if (
        !t?.closest("[data-focus-zone='panel']")
        && appStore.get(focusZoneAtom) !== "panel"
      ) return;
      if (e.code === "KeyA") {
        e.preventDefault();
        document.dispatchEvent(new CustomEvent("nergal:approve-plan"));
      } else if (e.code === "KeyC") {
        e.preventDefault();
        document.dispatchEvent(new CustomEvent("nergal:toggle-global-comment"));
      } else if (e.code === "KeyR") {
        if (annotations.length === 0) return;
        e.preventDefault();
        document.dispatchEvent(new CustomEvent("nergal:revise-plan"));
      } else if (e.code === "KeyX") {
        if (annotations.length === 0) return;
        e.preventDefault();
        void confirm({
          kind: "warning",
          destructive: true,
          title: "Clear annotations?",
          body: `Remove all ${annotations.length} annotation${annotations.length === 1 ? "" : "s"} on this plan?`,
          confirmLabel: "Clear",
        }).then((ok) => {
          if (ok) document.dispatchEvent(new CustomEvent("nergal:clear-annotations"));
        });
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [canAnnotate, annotations.length]);

  if (!hasPlan) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1">
        <span className="text-[11px] text-muted-foreground">No plan yet</span>
        <button
          onClick={() => setSettingsOpen(true)}
          className="text-[11px] text-muted-foreground/60 hover:text-muted-foreground transition-colors"
        >
          No plans found · set the plans directory in Settings
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header: annotation toggle + review actions */}
      <div className="flex h-8 shrink-0 items-center border-b border-border/50 px-2">
        <TooltipProvider delay={0}>
        <div className="flex flex-1 items-center justify-end gap-1.5">
          {canAnnotate && (
            <Tooltip>
              <TooltipTrigger>
                <button
                  onClick={() => setAnnotationMode((prev) => !prev)}
                  className={`flex h-5 items-center gap-1 rounded px-1.5 text-[10px] transition-colors ${
                    annotationMode
                      ? "bg-primary/15 text-primary"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
                >
                  <Highlighter size={11} className={annotationMode ? "animate-pulse" : ""} />
                  {annotationMode && <span className="text-[9px] font-medium uppercase tracking-wider">Annotate</span>}
                </button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="text-[10px]">Toggle annotation mode (Ctrl+Shift+H)</TooltipContent>
            </Tooltip>
          )}

          {isSubmitted && (
            <span className="text-[10px] text-muted-foreground">Waiting for revision...</span>
          )}

          {canAnnotate && (
            <>
              <span className="text-[10px] text-muted-foreground tabular-nums">
                {annotations.length}
              </span>
              <Tooltip>
                <TooltipTrigger>
                  <button
                    onClick={handleGlobalComment}
                    className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  >
                    <MessageSquare size={11} />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Global comment (C)</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger>
                  <button
                    onClick={() => {
                      void confirm({
                        kind: "warning",
                        destructive: true,
                        title: "Clear annotations?",
                        body: `Remove all ${annotations.length} annotation${annotations.length === 1 ? "" : "s"} on this plan?`,
                        confirmLabel: "Clear",
                      }).then((ok) => {
                        if (!ok) return;
                        clearAnnotations();
                        addToast({ message: "Cleared", description: "All annotations removed", type: "info" });
                      });
                    }}
                    className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  >
                    <Trash2 size={11} />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Clear all (X)</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger>
                  <button
                    onClick={handleApprove}
                    className="h-5 rounded border border-border px-2 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  >
                    Approve
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Approve plan (A)</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger>
                  <button
                    onClick={handleRevise}
                    disabled={annotations.length === 0}
                    className={`h-5 rounded px-2 text-[10px] font-medium transition-colors ${
                      annotations.length > 0
                        ? "bg-primary text-primary-foreground hover:bg-primary/90"
                        : "bg-secondary text-muted-foreground cursor-not-allowed"
                    }`}
                  >
                    Revise
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Revise with annotations (R)</TooltipContent>
              </Tooltip>
            </>
          )}
        </div>
        </TooltipProvider>
      </div>

      {/* Global comment input */}
      {showGlobalInput && canAnnotate && (
        <div className="border-b border-border/50 p-2">
          <textarea
            ref={globalInputRef}
            value={globalComment}
            onChange={(e) => setGlobalComment(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && e.ctrlKey) { e.preventDefault(); handleGlobalComment(); }
              if (e.key === "Escape") { setShowGlobalInput(false); setGlobalComment(""); }
            }}
            placeholder="General comment about this plan... (@@ to cite a vault note)"
            className="mb-1.5 h-14 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring outline-none"
          />
          {mentionOverlay}
          <div className="flex items-center justify-end gap-1">
            <button onClick={() => { setShowGlobalInput(false); setGlobalComment(""); }} className="rounded px-2 py-0.5 text-[10px] text-muted-foreground hover:text-foreground">
              Cancel
            </button>
            <button
              onClick={handleGlobalComment}
              disabled={!globalComment.trim()}
              className="rounded bg-primary px-2 py-0.5 text-[10px] font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              Add (Ctrl+Enter)
            </button>
          </div>
        </div>
      )}

      <div className="relative flex-1 overflow-y-auto">
        <AnnotatableMarkdownView
          content={plan.content}
          annotationsEnabled={canAnnotate}
          annotationMode={annotationMode}
        />
      </div>
    </div>
  );
}
