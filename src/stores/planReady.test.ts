import { describe, it, expect } from "vitest";
import {
  applyPlanReady,
  applyReviewClosed,
  decisionGuard,
  documentBase,
  isLiveReview,
  isReviewDocument,
  showReviewAnnotations,
} from "@/stores/planReady";
import { defaultPlanState, type PlanReviewStatus, type PlanState } from "@/stores/plan";

const stored: PlanState = {
  ...defaultPlanState,
  content: "# A",
  original: "# A",
  path: "/p/a.md",
  mode: "edit",
  claudeSessionId: "claude-1",
  decisionPath: "/run/user/1000/nergal/plan-1.fifo",
};

describe("applyPlanReady", () => {
  it("opens a review when the payload carries a decision path", () => {
    const r = applyPlanReady(undefined, undefined, {
      path: "/p/a.md",
      content: "# A",
      session_id: "claude-1",
      decision_path: "/fifo/new",
    });
    expect(r.status).toBe("pending_review");
    expect(r.plan.decisionPath).toBe("/fifo/new");
    expect(r.plan.path).toBe("/p/a.md");
    expect(r.plan.content).toBe("# A");
  });

  it("keeps the review cycle when a preview repeats the stored original", () => {
    const r = applyPlanReady(stored, "pending_review", {
      path: "/p/b.md",
      content: "# A",
      session_id: "claude-2",
    });
    expect(r.status).toBe("pending_review");
    expect(r.plan.decisionPath).toBe(stored.decisionPath);
    expect(r.plan.claudeSessionId).toBe("claude-1");
    expect(r.plan.mode).toBe("edit");
    expect(r.plan.path).toBe("/p/b.md");
  });

  it("keeps edited content when the preview equals the original", () => {
    const edited = { ...stored, content: "# A edited" };
    const r = applyPlanReady(edited, "pending_review", {
      path: "/p/a.md",
      content: "# A",
      session_id: "claude-1",
    });
    expect(r.plan.content).toBe("# A edited");
    expect(r.plan.decisionPath).toBe(stored.decisionPath);
    expect(r.status).toBe("pending_review");
  });

  it("starts a new idle cycle when the content differs", () => {
    const r = applyPlanReady(stored, "pending_review", {
      path: "/p/a.md",
      content: "# B",
      session_id: "claude-1",
    });
    expect(r.plan.content).toBe("# B");
    expect(r.plan.decisionPath).toBe("");
    expect(r.status).toBe("idle");
  });

  it("is idle when there is no stored plan and no decision path", () => {
    const r = applyPlanReady(undefined, undefined, {
      path: "/p/a.md",
      content: "# A",
      session_id: "claude-1",
    });
    expect(r.status).toBe("idle");
    expect(r.plan.decisionPath).toBe("");
    expect(r.plan.content).toBe("# A");
  });
});

describe("decisionGuard", () => {
  const ok = {
    repeat: false,
    inFlight: false,
    decisionPath: "/fifo",
    sessionId: "s1",
    status: "pending_review" as PlanReviewStatus,
  };

  it("ignores key repeats", () => {
    expect(decisionGuard({ ...ok, repeat: true })).toBe("ignore");
  });

  it("ignores a second decision while one is in flight", () => {
    expect(decisionGuard({ ...ok, inFlight: true })).toBe("ignore");
  });

  it("fails on an empty decision path", () => {
    expect(decisionGuard({ ...ok, decisionPath: "" })).toBe("fail");
  });

  it("fails on an empty session id", () => {
    expect(decisionGuard({ ...ok, sessionId: "" })).toBe("fail");
  });

  it("sends otherwise", () => {
    expect(decisionGuard(ok)).toBe("send");
  });

  it.each(["submitted", "idle", "undelivered"] as const)("ignores status %s even with a valid path and session", (status) => {
    expect(decisionGuard({ ...ok, status })).toBe("ignore");
  });
});

describe("documentBase", () => {
  const payload = { path: "/p/a.md", content: "# A", session_id: "claude-1" };
  const sessionPlan: PlanState = {
    ...stored,
    original: "# A",
    content: "# A",
    decisionPath: "/fifo/live",
  };

  it("falls back to the session plan when the stored document is stale", () => {
    const staleDoc: PlanState = { ...stored, original: "# OLD", content: "# OLD", decisionPath: "" };
    const base = documentBase(staleDoc, sessionPlan, payload);
    expect(base).toBe(sessionPlan);
    const r = applyPlanReady(base, "pending_review", payload);
    expect(r.plan.decisionPath).toBe("/fifo/live");
  });

  it("keeps the document when its original matches the payload", () => {
    const doc: PlanState = { ...stored, original: "# A", content: "# A edited" };
    expect(documentBase(doc, sessionPlan, payload)).toBe(doc);
  });

  it("uses the session plan when there is no document", () => {
    expect(documentBase(undefined, sessionPlan, payload)).toBe(sessionPlan);
  });

  it("is undefined when there is neither", () => {
    expect(documentBase(undefined, undefined, payload)).toBeUndefined();
  });
});

describe("applyPlanReady after an undelivered decision", () => {
  it("stays undelivered when a preview repeats the stored original", () => {
    const r = applyPlanReady(stored, "undelivered", {
      path: "/p/a.md",
      content: "# A",
      session_id: "claude-1",
    });
    expect(r.status).toBe("undelivered");
  });

  it("returns to pending_review when the payload carries a decision path", () => {
    const r = applyPlanReady(stored, "undelivered", {
      path: "/p/a.md",
      content: "# A",
      session_id: "claude-1",
      decision_path: "/fifo/retry",
    });
    expect(r.status).toBe("pending_review");
    expect(r.plan.decisionPath).toBe("/fifo/retry");
  });
});

describe("isLiveReview", () => {
  const live: PlanState = { ...stored, decisionPath: "/fifo/a" };

  it("is true for a pending review with matching paths", () => {
    expect(isLiveReview(live, live, "pending_review")).toBe(true);
  });

  it("is false when the document has no decision path", () => {
    expect(isLiveReview({ ...live, decisionPath: "" }, live, "pending_review")).toBe(false);
  });

  it("is false when the document path is stale", () => {
    expect(isLiveReview({ ...live, decisionPath: "/fifo/old" }, live, "pending_review")).toBe(false);
  });

  it.each(["submitted", "undelivered", "idle"] as const)("is false for status %s", (status) => {
    expect(isLiveReview(live, live, status)).toBe(false);
  });

  it("is false when the document plan is undefined", () => {
    expect(isLiveReview(undefined, live, "pending_review")).toBe(false);
  });

  it("is false when the session plan is undefined", () => {
    expect(isLiveReview(live, undefined, "pending_review")).toBe(false);
  });
});

describe("isReviewDocument", () => {
  const live: PlanState = { ...stored, decisionPath: "/fifo/a" };

  it("is true when both decision paths are equal and non-empty", () => {
    expect(isReviewDocument(live, { ...live })).toBe(true);
  });

  it("is false when the document decision path is empty", () => {
    expect(isReviewDocument({ ...live, decisionPath: "" }, live)).toBe(false);
  });

  it("is false when the paths differ", () => {
    expect(isReviewDocument({ ...live, decisionPath: "/fifo/old" }, live)).toBe(false);
  });

  it("is false when the document is undefined", () => {
    expect(isReviewDocument(undefined, live)).toBe(false);
  });

  it("is false when the session plan is undefined", () => {
    expect(isReviewDocument(live, undefined)).toBe(false);
  });
});

describe("showReviewAnnotations", () => {
  const live: PlanState = { ...stored, decisionPath: "/fifo/a" };

  it.each(["pending_review", "undelivered"] as const)("is true for a review document with status %s", (status) => {
    expect(showReviewAnnotations(live, live, status)).toBe(true);
  });

  it.each(["submitted", "idle"] as const)("is false for a review document with status %s", (status) => {
    expect(showReviewAnnotations(live, live, status)).toBe(false);
  });

  it("is false for an old document with a different decision path", () => {
    expect(showReviewAnnotations({ ...live, decisionPath: "/fifo/old" }, live, "pending_review")).toBe(false);
  });
});

describe("applyReviewClosed", () => {
  it.each([
    ["pending_review", "answered", null],
    ["undelivered", "answered", null],
    ["submitted", "answered", "Revise not delivered"],
    ["idle", "idle", null],
    ["answered", "answered", null],
  ] as const)("maps %s to %s with toast %s", (from, status, toast) => {
    expect(applyReviewClosed(from)).toEqual({ status, toast });
  });
});

describe("answered review", () => {
  const live: PlanState = { ...stored, decisionPath: "/fifo/a" };

  it("keeps annotations on the review document", () => {
    expect(showReviewAnnotations(live, live, "answered")).toBe(true);
  });

  it("hides annotations on a document with a different decision path", () => {
    expect(showReviewAnnotations({ ...live, decisionPath: "/fifo/old" }, live, "answered")).toBe(false);
  });

  it("ignores a decision once answered", () => {
    expect(
      decisionGuard({
        repeat: false,
        inFlight: false,
        decisionPath: "/fifo/a",
        sessionId: "s1",
        status: "answered",
      }),
    ).toBe("ignore");
  });
});
