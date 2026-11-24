import { describe, it, expect } from "vitest";
import {
  applyReviewClosed,
  applyReviewUnresolved,
  shouldRestoreAnnotations,
  shouldClearAnnotationsOnReview,
  statusOnStop,
} from "@/stores/planReady";
import type { PlanReviewStatus } from "@/stores/plan";

const all: PlanReviewStatus[] = ["idle", "pending_review", "submitted", "undelivered", "answered"];

describe("applyReviewClosed while a decision is in flight", () => {
  it.each(all)("leaves %s untouched with no toast", (status) => {
    expect(applyReviewClosed(status, true)).toEqual({ status, toast: null });
  });
});

describe("applyReviewClosed with nothing in flight", () => {
  it.each([
    ["pending_review", "answered", null],
    ["undelivered", "answered", null],
    ["submitted", "answered", "Revise not delivered"],
    ["idle", "idle", null],
    ["answered", "answered", null],
  ] as const)("maps %s to %s with toast %s", (from, status, toast) => {
    expect(applyReviewClosed(from, false)).toEqual({ status, toast });
  });

  it("defaults inFlight to false", () => {
    expect(applyReviewClosed("submitted")).toEqual({
      status: "answered",
      toast: "Revise not delivered",
    });
  });
});

describe("shouldRestoreAnnotations", () => {
  it("restores only when a submitted Revise turns out undelivered", () => {
    expect(shouldRestoreAnnotations("submitted", "answered")).toBe(true);
  });

  it.each([
    ["pending_review", "answered"],
    ["undelivered", "answered"],
    ["submitted", "submitted"],
    ["idle", "idle"],
  ] as const)("does not restore %s to %s", (prev, next) => {
    expect(shouldRestoreAnnotations(prev, next)).toBe(false);
  });
});

describe("statusOnStop", () => {
  it("keeps answered so the terminal answer stays visible", () => {
    expect(statusOnStop("answered")).toBe("answered");
  });

  it.each(["pending_review", "submitted", "undelivered", "idle"] as const)(
    "resets %s to idle",
    (status) => {
      expect(statusOnStop(status)).toBe("idle");
    },
  );
});

describe("applyReviewUnresolved", () => {
  it("returns a submitted review to idle", () => {
    expect(applyReviewUnresolved("submitted")).toBe("idle");
  });

  it.each(["idle", "pending_review", "undelivered", "answered"] as const)(
    "leaves %s unchanged",
    (status) => {
      expect(applyReviewUnresolved(status)).toBe(status);
    },
  );
});

describe("shouldClearAnnotationsOnReview", () => {
  it("clears when a new decision path arrives", () => {
    expect(shouldClearAnnotationsOnReview("/fifo/a", "/fifo/b")).toBe(true);
    expect(shouldClearAnnotationsOnReview(undefined, "/fifo/b")).toBe(true);
    expect(shouldClearAnnotationsOnReview("", "/fifo/b")).toBe(true);
  });

  it("keeps annotations when the path repeats", () => {
    expect(shouldClearAnnotationsOnReview("/fifo/a", "/fifo/a")).toBe(false);
  });

  it("keeps annotations when the next path is empty", () => {
    expect(shouldClearAnnotationsOnReview("/fifo/a", "")).toBe(false);
    expect(shouldClearAnnotationsOnReview("/fifo/a", undefined)).toBe(false);
  });
});
