import { describe, expect, it } from "vitest";
import { PR_FALLBACK_MS, prSignature, shouldRefreshPr } from "@/stores/prSignal";

const NOW = 1_000_000;

function entry(ageMs: number, signal?: string) {
  return { data: null, fetchedAt: NOW - ageMs, signal };
}

describe("prSignature", () => {
  it("is none without a PR", () => {
    expect(prSignature(null)).toBe("none");
    expect(prSignature(undefined)).toBe("none");
  });

  it("is pr:<number> for a GitHub PR", () => {
    expect(prSignature({ number: 12, url: null, review_state: null, kind: null })).toBe("pr:12");
  });

  it("ignores review_state", () => {
    const base = { number: 12, url: null, kind: null };
    expect(prSignature({ ...base, review_state: "approved" })).toBe(
      prSignature({ ...base, review_state: "pending" }),
    );
  });

  it("is none for a GitLab merge request", () => {
    expect(prSignature({ number: 12, url: null, review_state: null, kind: "mr" })).toBe("none");
  });
});

describe("shouldRefreshPr", () => {
  it("refreshes when there is no entry", () => {
    expect(shouldRefreshPr(undefined, "none", NOW)).toBe(true);
    expect(shouldRefreshPr(undefined, undefined, NOW)).toBe(true);
  });

  it("skips a recent entry with the same signal", () => {
    expect(shouldRefreshPr(entry(10_000, "pr:12"), "pr:12", NOW)).toBe(false);
  });

  it("refreshes when the signal differs", () => {
    expect(shouldRefreshPr(entry(10_000, "none"), "pr:12", NOW)).toBe(true);
  });

  it("refreshes once the entry is older than 60 s", () => {
    expect(shouldRefreshPr(entry(61_000, "pr:12"), "pr:12", NOW)).toBe(true);
  });

  it("does not refresh at exactly 60 s", () => {
    expect(shouldRefreshPr(entry(60_000, "pr:12"), "pr:12", NOW)).toBe(false);
  });

  it("without any signal on either side refreshes only past 60 s", () => {
    expect(shouldRefreshPr(entry(10_000), undefined, NOW)).toBe(false);
    expect(shouldRefreshPr(entry(61_000), undefined, NOW)).toBe(true);
  });

  it("refreshes an unsignaled entry once a signal is defined", () => {
    expect(shouldRefreshPr(entry(1_000), "pr:12", NOW)).toBe(true);
    expect(shouldRefreshPr(entry(1_000), "none", NOW)).toBe(true);
  });

  it("refreshes on mount after a change while another session was shown", () => {
    expect(shouldRefreshPr(entry(10_000, "none"), "pr:12", NOW)).toBe(true);
  });

  it("exports the 60 s fallback", () => {
    expect(PR_FALLBACK_MS).toBe(60_000);
  });
});
