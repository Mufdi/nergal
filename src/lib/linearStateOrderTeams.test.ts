import { describe, expect, it } from "vitest";
import { groupByState, stateFilterOptions, type StateLike } from "./linearStateOrder";

const issue = (stateName: string, stateType: string, statePosition: number): StateLike => ({
  stateName,
  stateType,
  statePosition,
});

const reviewLow = issue("In Review", "started", 1);
const reviewHigh = issue("In Review", "started", 1002);
const progress = issue("In Progress", "started", 2);

// Teams share a state name with different positions; the merged group must
// rank by its best member, not by whichever issue arrived first.
const ORDERS: Record<string, StateLike[]> = {
  "low-position team first": [reviewLow, progress, reviewHigh],
  "high-position team first": [reviewHigh, progress, reviewLow],
};

describe("state ordering with a state name shared across teams", () => {
  it.each(Object.entries(ORDERS))("groupByState puts In Review first (%s)", (_, issues) => {
    expect(groupByState(issues).map((g) => g.label)).toEqual(["In Review", "In Progress"]);
  });

  it.each(Object.entries(ORDERS))("stateFilterOptions puts In Review first (%s)", (_, issues) => {
    expect(stateFilterOptions(issues).map((o) => o.name)).toEqual(["In Review", "In Progress"]);
  });

  it("produces the same group order for both input orders", () => {
    const [a, b] = Object.values(ORDERS).map((o) => groupByState(o).map((g) => g.label));
    expect(a).toEqual(b);
  });
});
