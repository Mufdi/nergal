import { describe, expect, it } from "vitest";
import {
  MARKDOWN_CONTAINER_CLASS,
  markdownTableClass,
  markdownWidthClass,
} from "./readingWidth";

describe("markdownWidthClass", () => {
  it("caps at 100ch and centers when reading mode is on", () => {
    const classes = markdownWidthClass(true).split(" ");
    expect(classes).toContain("max-w-[100ch]");
    expect(classes).toContain("mx-auto");
    expect(classes).not.toContain("max-w-none");
    expect(classes).not.toContain("max-w-[72ch]");
  });

  it("keeps the full width when reading mode is off", () => {
    expect(markdownWidthClass(false)).toBe("max-w-none");
  });
});

describe("MARKDOWN_CONTAINER_CLASS", () => {
  it("is the container-query root", () => {
    expect(MARKDOWN_CONTAINER_CLASS).toBe("@container");
  });
});

describe("markdownTableClass", () => {
  it("breaks tables out to the container width when full width is on", () => {
    const classes = markdownTableClass(true).split(" ");
    expect(classes).toContain("w-[calc(100cqw_-_2rem)]");
    expect(classes).toContain("ml-[calc(50%_-_50cqw_+_1rem)]");
    expect(classes).not.toContain("w-full");
    expect(classes).toContain("border-collapse");
  });

  it("keeps tables at the column width when full width is off", () => {
    const classes = markdownTableClass(false).split(" ");
    expect(classes).toContain("w-full");
    expect(classes).not.toContain("w-[calc(100cqw_-_2rem)]");
    expect(classes).not.toContain("ml-[calc(50%_-_50cqw_+_1rem)]");
    expect(classes).toContain("border-collapse");
  });
});
