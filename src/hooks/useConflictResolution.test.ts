import { describe, it, expect } from "vitest";
import {
  parseRegions,
  applyChoice,
  hasConflictMarkers,
  clampRegionIndex,
  wrapPickerIndex,
} from "@/hooks/useConflictResolution";

describe("parseRegions", () => {
  it("finds a single conflict region", () => {
    const text = ["a", "<<<<<<< HEAD", "ours1", "ours2", "=======", "theirs1", ">>>>>>> branch", "b"].join("\n");
    const regions = parseRegions(text);
    expect(regions).toHaveLength(1);
    expect(regions[0]).toMatchObject({
      start: 1,
      sep: 4,
      end: 6,
      oursLines: ["ours1", "ours2"],
      theirsLines: ["theirs1"],
    });
  });

  it("finds multiple regions in order", () => {
    const text = [
      "<<<<<<< HEAD", "o1", "=======", "t1", ">>>>>>> branch",
      "shared",
      "<<<<<<< HEAD", "o2", "=======", "t2", ">>>>>>> branch",
    ].join("\n");
    const regions = parseRegions(text);
    expect(regions).toHaveLength(2);
    expect(regions[0].oursLines).toEqual(["o1"]);
    expect(regions[1].oursLines).toEqual(["o2"]);
  });

  it("returns no regions for clean text", () => {
    expect(parseRegions("a\nb\nc")).toEqual([]);
  });
});

describe("applyChoice", () => {
  const text = ["a", "<<<<<<< HEAD", "ours1", "=======", "theirs1", ">>>>>>> branch", "b"].join("\n");
  const region = parseRegions(text)[0];

  it("keeps ours lines only", () => {
    expect(applyChoice(text, region, "ours")).toBe("a\nours1\nb");
  });

  it("keeps theirs lines only", () => {
    expect(applyChoice(text, region, "theirs")).toBe("a\ntheirs1\nb");
  });

  it("keeps both, ours then theirs", () => {
    expect(applyChoice(text, region, "both")).toBe("a\nours1\ntheirs1\nb");
  });
});

describe("hasConflictMarkers", () => {
  it("detects an unresolved marker", () => {
    expect(hasConflictMarkers("a\n<<<<<<< HEAD\nb")).toBe(true);
    expect(hasConflictMarkers("a\n=======\nb")).toBe(true);
    expect(hasConflictMarkers("a\n>>>>>>> branch\nb")).toBe(true);
  });

  it("reports clean text as marker-free", () => {
    expect(hasConflictMarkers("a\nb\nc")).toBe(false);
  });
});

describe("clampRegionIndex", () => {
  it("advances forward without exceeding the last region", () => {
    expect(clampRegionIndex(0, 3, "next")).toBe(1);
    expect(clampRegionIndex(2, 3, "next")).toBe(2);
  });

  it("retreats without going below zero", () => {
    expect(clampRegionIndex(1, 3, "prev")).toBe(0);
    expect(clampRegionIndex(0, 3, "prev")).toBe(0);
  });
});

describe("wrapPickerIndex", () => {
  it("wraps forward past the last file", () => {
    expect(wrapPickerIndex(2, 3, "next")).toBe(0);
  });

  it("wraps backward past the first file", () => {
    expect(wrapPickerIndex(0, 3, "prev")).toBe(2);
  });

  it("returns 0 for an empty file list", () => {
    expect(wrapPickerIndex(0, 0, "next")).toBe(0);
  });
});
