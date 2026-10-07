import { describe, expect, it } from "vitest";
import {
  compareStates,
  groupByState,
  isClosedState,
  normalizeStateName,
  stateFilterOptions,
  stateRank,
  type StateLike,
} from "./linearStateOrder";

const s = (stateName: string, stateType: string, statePosition = 0): StateLike => ({
  stateName,
  stateType,
  statePosition,
});

const TEAM_RR: StateLike[] = [
  s("Triage", "triage", 0),
  s("Backlog", "backlog", 0),
  s("Todo", "unstarted", 0),
  s("In Progress", "started", 1),
  s("In Review", "started", 2),
  s("Pending", "completed", -899),
  s("Done", "completed", 3),
  s("Duplicate", "canceled", 5),
  s("Canceled", "canceled", 4),
];

describe("stateRank / compareStates", () => {
  it("orders active work first and closed states last", () => {
    const sorted = [...TEAM_RR].sort(compareStates).map((x) => x.stateName);
    expect(sorted).toEqual([
      "Pending",
      "In Review",
      "In Progress",
      "Todo",
      "Backlog",
      "Triage",
      "Done",
      "Duplicate",
      "Canceled",
    ]);
  });

  it("tells Duplicate and Canceled apart by name though both are type canceled", () => {
    expect(stateRank(s("Duplicate", "canceled"))).toBeLessThan(stateRank(s("Canceled", "canceled")));
  });

  it("ranks Pending with In Review and treats it as open", () => {
    expect(stateRank(s("Pending", "completed"))).toBe(stateRank(s("In Review", "started")));
    expect(isClosedState(s("Pending", "completed"))).toBe(false);
  });

  it("matches names case-insensitively and accepts the cancelled spelling", () => {
    expect(stateRank(s("in review", "started"))).toBe(stateRank(s("In Review", "started")));
    expect(stateRank(s("Cancelled", "cancelled"))).toBe(stateRank(s("Canceled", "canceled")));
  });

  it("falls back to state type for unknown names", () => {
    expect(stateRank(s("QA", "started"))).toBe(stateRank(s("In Progress", "started")));
    expect(stateRank(s("Shipped", "completed"))).toBe(stateRank(s("Done", "completed")));
    expect(stateRank(s("Icebox", "backlog"))).toBe(stateRank(s("Backlog", "backlog")));
    expect(stateRank(s("Dup", "duplicate"))).toBe(stateRank(s("Duplicate", "canceled")));
    expect(stateRank(s("Nope", "canceled"))).toBe(stateRank(s("Canceled", "canceled")));
  });

  it("ranks an unknown open type after the named open states but above closed", () => {
    const weird = s("Mystery", "something");
    expect(stateRank(weird)).toBeGreaterThan(stateRank(s("Triage", "triage")));
    expect(stateRank(weird)).toBeLessThan(stateRank(s("Done", "completed")));
    expect(isClosedState(weird)).toBe(false);
  });

  it("never interleaves a closed state between open ones, whatever the position", () => {
    const open = s("Backlog", "backlog", 999);
    const closed = s("Done", "completed", -999);
    expect(compareStates(open, closed)).toBeLessThan(0);
  });

  it("breaks ties by workflow position ascending", () => {
    expect(compareStates(s("QA", "started", 5), s("In Progress", "started", 1))).toBeGreaterThan(0);
  });

  it("sorts issues without a state last", () => {
    const none: StateLike = {};
    expect(compareStates(s("Canceled", "canceled"), none)).toBeLessThan(0);
    expect(isClosedState(none)).toBe(false);
  });
});

describe("isClosedState", () => {
  it("classifies Done, Duplicate and Canceled as closed", () => {
    expect(isClosedState(s("Done", "completed"))).toBe(true);
    expect(isClosedState(s("Duplicate", "canceled"))).toBe(true);
    expect(isClosedState(s("Canceled", "canceled"))).toBe(true);
    expect(isClosedState(s("In Review", "started"))).toBe(false);
    expect(isClosedState(s("Triage", "triage"))).toBe(false);
  });
});

describe("state visibility filter", () => {
  it("normalizes names", () => {
    expect(normalizeStateName("  In Review ")).toBe("in review");
    expect(normalizeStateName(undefined)).toBe("");
  });

  it("lists deduplicated options in state rank order", () => {
    const issues = [
      { ...s("Backlog", "backlog"), stateColor: "#aaa" },
      { ...s("Canceled", "canceled"), stateColor: "#bbb" },
      { ...s("In Review", "started"), stateColor: "#ccc" },
      { ...s("backlog", "backlog"), stateColor: "#ddd" },
    ];
    const opts = stateFilterOptions(issues);
    expect(opts.map((o) => o.key)).toEqual(["in review", "backlog", "canceled"]);
    expect(opts[1]).toMatchObject({ name: "Backlog", stateType: "backlog", color: "#aaa" });
  });

  it("skips issues without a state name in the options", () => {
    expect(stateFilterOptions([{ stateType: "started" }])).toEqual([]);
  });
});

describe("groupByState", () => {
  const issue = (id: string, name: string | undefined, type: string | undefined, pos = 0) => ({
    id,
    stateName: name,
    stateType: type,
    statePosition: pos,
    stateColor: `#${id}`,
  });

  it("orders sub-groups by state rank, closed last", () => {
    const groups = groupByState([
      issue("1", "Done", "completed"),
      issue("2", "Triage", "triage"),
      issue("3", "In Review", "started", 2),
      issue("4", "In Progress", "started", 1),
      issue("5", "Todo", "unstarted"),
      issue("6", "Backlog", "backlog"),
    ]);
    expect(groups.map((g) => g.label)).toEqual([
      "In Review",
      "In Progress",
      "Todo",
      "Backlog",
      "Triage",
      "Done",
    ]);
  });

  it("merges same-named states and keeps issue order inside a sub-group", () => {
    const groups = groupByState([
      issue("a", "Todo", "unstarted"),
      issue("b", "todo", "unstarted"),
      issue("c", "Todo", "unstarted"),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].key).toBe("todo");
    expect(groups[0].issues.map((i) => i.id)).toEqual(["a", "b", "c"]);
    expect(groups[0]).toMatchObject({ label: "Todo", stateType: "unstarted", color: "#a" });
  });

  it("puts issues without a state in a trailing No state group", () => {
    const groups = groupByState([issue("1", undefined, undefined), issue("2", "Done", "completed")]);
    expect(groups.map((g) => g.label)).toEqual(["Done", "No state"]);
    expect(groups[1]).toMatchObject({ key: "", stateType: null });
  });

  it("returns no groups for no issues", () => {
    expect(groupByState([])).toEqual([]);
  });
});
