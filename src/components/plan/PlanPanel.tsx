import { useState, useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import { planDocumentsAtom, defaultPlanState, setPlanDocModeAtom, activePlanReviewStatusAtom, planReviewStatusMapAtom } from "@/stores/plan";
import { activeSessionIdAtom } from "@/stores/workspace";
import { activeAnnotationsAtom, clearAnnotationsAtom, addAnnotationAtom, serializeAnnotations } from "@/stores/annotations";
import { toastsAtom } from "@/stores/toast";
import { invoke } from "@/lib/tauri";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import type { PlanMode } from "@/lib/types";
import { AnnotatableMarkdownView } from "./AnnotatableMarkdownView";
import { PlanEditor } from "./PlanEditor";
import { MessageSquare, Trash2 } from "lucide-react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

interface PlanPanelProps {
  path: string;
}

export function PlanPanel({ path }: PlanPanelProps) {
  const docs = useAtomValue(planDocumentsAtom);
  const plan = path ? (docs[path] ?? defaultPlanState) : defaultPlanState;
  const setMode = useSetAtom(setPlanDocModeAtom);
  const addToast = useSetAtom(toastsAtom);
  const annotations = useAtomValue(activeAnnotationsAtom);
  const clearAnnotations = useSetAtom(clearAnnotationsAtom);
  const addAnnotation = useSetAtom(addAnnotationAtom);
  const reviewStatus = useAtomValue(activePlanReviewStatusAtom);
  const setReviewStatusMap = useSetAtom(planReviewStatusMapAtom);
  const sessionId = useAtomValue(activeSessionIdAtom);
  const canAnnotate = reviewStatus === "pending_review";
  const isSubmitted = reviewStatus === "submitted";

  const [showGlobalInput, setShowGlobalInput] = useState(false);
  const [globalComment, setGlobalComment] = useState("");
  const globalInputRef = useRef<HTMLTextAreaElement>(null);

  const hasPlan = plan.content.length > 0;
  const hasEdits = plan.content !== plan.original;
  const backendSessionId = plan.claudeSessionId;
  const decisionPath = plan.decisionPath;

  useEffect(() => {
    if (showGlobalInput) globalInputRef.current?.focus();
  }, [showGlobalInput]);

  useEffect(() => {
    function handleToggleGlobal() { setShowGlobalInput((prev) => !prev); }
    document.addEventListener("cluihud:toggle-global-comment", handleToggleGlobal);
    return () => document.removeEventListener("cluihud:toggle-global-comment", handleToggleGlobal);
  }, []);

  function handleSave() {
    if (!plan.path || !backendSessionId) {
      addToast({ message: "Error", description: "No plan path — cannot save", type: "error" });
      return;
    }
    invoke("save_plan", { sessionId: backendSessionId, content: plan.content })
      .then(() => addToast({ message: "Plan Saved", description: "Edits saved to disk", type: "success" }))
      .catch((err: unknown) => addToast({ message: "Save Failed", description: String(err), type: "error" }));
  }

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
    if (!canAnnotate || plan.mode !== "view") return;

    function onClear() {
      clearAnnotations();
      addToast({ message: "Cleared", description: "All annotations removed", type: "info" });
    }
    function onApprove() { handleApprove(); }
    function onRevise() { if (annotations.length > 0) handleRevise(); }

    document.addEventListener("cluihud:clear-annotations", onClear);
    document.addEventListener("cluihud:approve-plan", onApprove);
    document.addEventListener("cluihud:revise-plan", onRevise);
    return () => {
      document.removeEventListener("cluihud:clear-annotations", onClear);
      document.removeEventListener("cluihud:approve-plan", onApprove);
      document.removeEventListener("cluihud:revise-plan", onRevise);
    };
  }, [canAnnotate, plan.mode, annotations.length, backendSessionId, decisionPath]);

  if (!hasPlan) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-[11px] text-muted-foreground">No plan yet</span>
      </div>
    );
  }

  return (
    <Tabs
      value={plan.mode}
      onValueChange={(value) => {
        if (canAnnotate && value === "edit") return;
        setMode({ path, mode: value as PlanMode });
      }}
      className="flex h-full flex-col gap-0"
    >
      {/* Header: View/Edit tabs + review actions */}
      <div className="flex h-8 shrink-0 items-center border-b border-border/50 px-2">
        <TabsList variant="line" className="h-full">
          <TabsTrigger value="view" className="text-[11px]">View</TabsTrigger>
          <TabsTrigger value="edit" className="text-[11px]" disabled={canAnnotate}>Edit</TabsTrigger>
        </TabsList>

        <div className="flex flex-1 items-center justify-end gap-1.5">
          {plan.mode === "edit" && (
            <button
              onClick={handleSave}
              disabled={!hasEdits}
              className={`h-5 rounded px-2 text-[10px] font-medium transition-colors ${
                hasEdits
                  ? "bg-primary text-primary-foreground hover:bg-primary/90"
                  : "bg-secondary text-muted-foreground cursor-not-allowed"
              }`}
            >
              Save
            </button>
          )}

          {plan.mode === "view" && isSubmitted && (
            <span className="text-[10px] text-muted-foreground">Waiting for revision...</span>
          )}

          {plan.mode === "view" && canAnnotate && (
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
                <TooltipContent side="bottom" className="text-[10px]">Global comment (Ctrl+Shift+O)</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger>
                  <button
                    onClick={() => { clearAnnotations(); addToast({ message: "Cleared", description: "All annotations removed", type: "info" }); }}
                    className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
                  >
                    <Trash2 size={11} />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="text-[10px]">Clear all (Ctrl+Shift+X)</TooltipContent>
              </Tooltip>
              <button
                onClick={handleApprove}
                className="h-5 rounded border border-border px-2 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground transition-colors"
              >
                Approve
              </button>
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
            </>
          )}
        </div>
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
            placeholder="General comment about this plan..."
            className="mb-1.5 h-14 w-full resize-none rounded border border-border bg-background p-2 text-xs focus:ring-1 focus:ring-ring outline-none"
          />
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

      <TabsContent value="view" className="flex-1 overflow-y-auto">
        <AnnotatableMarkdownView content={plan.content} annotationsEnabled={canAnnotate} />
      </TabsContent>

      <TabsContent value="edit" className="flex-1 overflow-hidden">
        <PlanEditor path={path} />
      </TabsContent>
    </Tabs>
  );
}
