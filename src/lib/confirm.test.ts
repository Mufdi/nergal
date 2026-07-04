import { describe, it, expect } from "vitest";
import { confirm, resolveConfirm, getActiveConfirm, enterKeyConfirms } from "@/lib/confirm";

describe("enterKeyConfirms (D5 anti-stray-Enter)", () => {
  it("defaults to true when enterConfirms is unset — every existing caller is unaffected", () => {
    expect(enterKeyConfirms({ title: "t" })).toBe(true);
  });

  it("is true when enterConfirms is explicitly true", () => {
    expect(enterKeyConfirms({ title: "t", enterConfirms: true })).toBe(true);
  });

  it("is false when enterConfirms is explicitly false (the deep-link variant)", () => {
    expect(enterKeyConfirms({ title: "t", enterConfirms: false })).toBe(false);
  });
});

describe("confirm() queue", () => {
  it("queues a second confirm behind a pending one instead of coalescing", async () => {
    const first = confirm({ title: "first" });
    const second = confirm({ title: "second" });

    // Only the first is active; the second must not bypass or replace it.
    expect(getActiveConfirm()?.opts.title).toBe("first");

    resolveConfirm(true);
    expect(await first).toBe(true);

    // Resolving the first advances the queue to the second, unmodified.
    expect(getActiveConfirm()?.opts.title).toBe("second");

    resolveConfirm(false);
    expect(await second).toBe(false);
    expect(getActiveConfirm()).toBeNull();
  });
});
