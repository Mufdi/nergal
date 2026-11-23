import { defaultPlanState, type PlanReviewStatus, type PlanState } from "@/stores/plan";

export interface PlanReadyPayload {
  path: string;
  content: string;
  session_id: string;
  decision_path?: string;
}

export function applyPlanReady(
  prevPlan: PlanState | undefined,
  prevStatus: PlanReviewStatus | undefined,
  payload: PlanReadyPayload,
): { plan: PlanState; status: PlanReviewStatus } {
  if (payload.decision_path) {
    return {
      plan: {
        ...defaultPlanState,
        content: payload.content,
        original: payload.content,
        path: payload.path,
        claudeSessionId: payload.session_id,
        decisionPath: payload.decision_path,
      },
      status: "pending_review",
    };
  }

  // Compared against `original`, never `content`: the user's edits live in `content`.
  if (prevPlan && prevPlan.original === payload.content) {
    return {
      plan: { ...prevPlan, path: payload.path },
      status: prevStatus ?? "idle",
    };
  }

  return {
    plan: {
      ...defaultPlanState,
      content: payload.content,
      original: payload.content,
      path: payload.path,
      claudeSessionId: payload.session_id,
    },
    status: "idle",
  };
}

// A preview can land on a path whose document is from an earlier cycle; only a
// document that matches the payload may stand in for the session's live review.
export function documentBase(
  prevDoc: PlanState | undefined,
  sessionPlan: PlanState | undefined,
  payload: PlanReadyPayload,
): PlanState | undefined {
  return prevDoc && prevDoc.original === payload.content ? prevDoc : sessionPlan;
}

export function decisionGuard(args: {
  repeat: boolean;
  inFlight: boolean;
  decisionPath: string;
  sessionId: string;
  status: PlanReviewStatus;
}): "ignore" | "fail" | "send" {
  if (args.repeat || args.inFlight || args.status !== "pending_review") return "ignore";
  if (!args.decisionPath || !args.sessionId) return "fail";
  return "send";
}

export function isReviewDocument(
  docPlan: PlanState | undefined,
  sessionPlan: PlanState | undefined,
): boolean {
  return !!docPlan?.decisionPath && docPlan.decisionPath === sessionPlan?.decisionPath;
}

export function isLiveReview(
  docPlan: PlanState | undefined,
  sessionPlan: PlanState | undefined,
  status: PlanReviewStatus,
): boolean {
  return status === "pending_review" && isReviewDocument(docPlan, sessionPlan);
}

export function showReviewAnnotations(
  docPlan: PlanState | undefined,
  sessionPlan: PlanState | undefined,
  status: PlanReviewStatus,
): boolean {
  return (
    (status === "pending_review" || status === "undelivered")
    && isReviewDocument(docPlan, sessionPlan)
  );
}
