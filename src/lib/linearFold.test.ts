import { describe, expect, it } from "vitest";
import { foldedByStateFilter, isGroupFolded, withoutPeeks, type FoldContext } from "./linearFold";
import type { StateLike } from "./linearStateOrder";

const s = (stateName: string, stateType: string): StateLike => ({ stateName, stateType });

const DONE = s("Done", "completed");
const TODO = s("Todo", "unstarted");
const SHIPPED = s("Shipped", "completed");
const PENDING_STARTED = s("Pending", "started");
const CANCELED = s("Canceled", "canceled");

function ctx(overrides: Partial<FoldContext> = {}): FoldContext {
  return {
    collapsed: new Set(),
    peeked: new Map(),
    hidden: new Set(),
    showCompleted: true,
    ...overrides,
  };
}

describe("foldedByStateFilter", () => {
  it("folds a state hidden in the dropdown, matched by normalized name", () => {
    expect(foldedByStateFilter(s(" done ", "completed"), new Set(["done"]), true)).toBe(true);
    expect(foldedByStateFilter(TODO, new Set(["done"]), true)).toBe(false);
  });

  it("folds every closed state when show-completed is off", () => {
    expect(foldedByStateFilter(DONE, new Set(), false)).toBe(true);
    expect(foldedByStateFilter(CANCELED, new Set(), false)).toBe(true);
    expect(foldedByStateFilter(TODO, new Set(), false)).toBe(false);
  });

  it("folds a Shipped completed state when show-completed is off", () => {
    expect(foldedByStateFilter(SHIPPED, new Set(), false)).toBe(true);
  });

  it("keeps a state named Pending of type started unfolded when show-completed is off", () => {
    expect(foldedByStateFilter(PENDING_STARTED, new Set(), false)).toBe(false);
  });

  it("never folds a group without a state", () => {
    expect(foldedByStateFilter(undefined, new Set(["done"]), false)).toBe(false);
    expect(foldedByStateFilter({}, new Set([""]), false)).toBe(false);
  });
});

describe("isGroupFolded", () => {
  it("folds a hidden-state group even when it is not in the collapsed set", () => {
    expect(isGroupFolded("state:1", DONE, ctx({ hidden: new Set(["done"]) }))).toBe(true);
  });

  it("folds closed-state groups and sub-groups when show-completed is off", () => {
    const c = ctx({ showCompleted: false });
    expect(isGroupFolded("state:1", DONE, c)).toBe(true);
    expect(isGroupFolded("project:a|state:done", DONE, c)).toBe(true);
    expect(isGroupFolded("state:2", TODO, c)).toBe(false);
  });

  it("unfolds a hidden-state group that was peeked", () => {
    const c = ctx({ hidden: new Set(["done"]), peeked: new Map([["state:1", DONE]]) });
    expect(isGroupFolded("state:1", DONE, c)).toBe(false);
    expect(isGroupFolded("project:a|state:done", DONE, c)).toBe(true);
  });

  it("ignores the collapsed set for a hidden-state group", () => {
    const c = ctx({
      hidden: new Set(["done"]),
      collapsed: new Set(["state:1"]),
      peeked: new Map([["state:1", DONE]]),
    });
    expect(isGroupFolded("state:1", DONE, c)).toBe(false);
  });

  it("follows the collapsed set for groups no filter folds", () => {
    expect(isGroupFolded("state:2", TODO, ctx({ collapsed: new Set(["state:2"]) }))).toBe(true);
    expect(isGroupFolded("state:2", TODO, ctx())).toBe(false);
    expect(isGroupFolded("project:a", undefined, ctx({ collapsed: new Set(["project:a"]) }))).toBe(true);
  });

  it("ignores a stale peek once the state is no longer folded by a filter", () => {
    const c = ctx({ collapsed: new Set(["state:1"]), peeked: new Map([["state:1", DONE]]) });
    expect(isGroupFolded("state:1", DONE, c)).toBe(true);
  });
});

describe("withoutPeeks", () => {
  it("drops only the peeks whose state matches", () => {
    const peeked = new Map<string, StateLike>([
      ["state:1", DONE],
      ["project:a|state:done", DONE],
      ["state:2", CANCELED],
    ]);
    const out = withoutPeeks(peeked, (st) => st.stateName === "Done");
    expect([...out.keys()]).toEqual(["state:2"]);
  });

  it("returns the same map when nothing matches", () => {
    const peeked = new Map<string, StateLike>([["state:2", CANCELED]]);
    expect(withoutPeeks(peeked, () => false)).toBe(peeked);
  });
});
