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
  s("Todo", "unstarted", 1),
  s("In Progress", "started", 2),
  s("Done", "completed", 3),
  s("Canceled", "canceled", 4),
  s("Duplicate", "duplicate", 5),
  s("Shipped", "completed", 954),
  s("In Review", "started", 1002),
];

const TEAM_RR_ORDER = [
  "In Review",
  "In Progress",
  "Todo",
  "Backlog",
  "Triage",
  "Done",
  "Shipped",
  "Duplicate",
  "Canceled",
];

describe("stateRank / compareStates", () => {
  it("sorts team RR's workflow from type and position alone", () => {
    const sorted = [...TEAM_RR].sort(compareStates).map((x) => x.stateName);
    expect(sorted).toEqual(TEAM_RR_ORDER);
  });

  it("is independent of input order", () => {
    const sorted = [...TEAM_RR].reverse().sort(compareStates).map((x) => x.stateName);
    expect(sorted).toEqual(TEAM_RR_ORDER);
  });

  it("ranks types started, unstarted, backlog, triage, unknown open, completed, duplicate, canceled", () => {
    const ranks = [
      s("a", "started"),
      s("a", "unstarted"),
      s("a", "backlog"),
      s("a", "triage"),
      s("a", "something-new"),
      s("a", "completed"),
      s("a", "duplicate"),
      s("a", "canceled"),
    ].map(stateRank);
    for (let i = 1; i < ranks.length; i++) {
      expect(ranks[i]).toBeGreaterThan(ranks[i - 1]);
    }
  });

  it("puts issues without a state after every typed state", () => {
    const none: StateLike = {};
    expect(compareStates(s("Canceled", "canceled", 999), none)).toBeLessThan(0);
    expect(compareStates(none, s("Canceled", "canceled", 999))).toBeGreaterThan(0);
  });

  it("ranks a higher position first inside an open type", () => {
    expect(compareStates(s("Code review", "started", 1002), s("In Progress", "started", 2))).toBeLessThan(0);
    expect(compareStates(s("A", "unstarted", 1), s("B", "unstarted", 9))).toBeGreaterThan(0);
    expect(compareStates(s("A", "backlog", 1), s("B", "backlog", 9))).toBeGreaterThan(0);
    expect(compareStates(s("A", "triage", 1), s("B", "triage", 9))).toBeGreaterThan(0);
    expect(compareStates(s("A", "weird", 1), s("B", "weird", 9))).toBeGreaterThan(0);
  });

  it("ranks a lower position first inside a closed type", () => {
    expect(compareStates(s("Done", "completed", 3), s("Shipped", "completed", 954))).toBeLessThan(0);
    expect(compareStates(s("A", "duplicate", 1), s("B", "duplicate", 9))).toBeLessThan(0);
    expect(compareStates(s("A", "canceled", 1), s("B", "canceled", 9))).toBeLessThan(0);
  });

  it("never lets state names affect the rank", () => {
    expect(stateRank(s("Pending", "completed"))).toBe(stateRank(s("Done", "completed")));
    expect(stateRank(s("In Review", "backlog"))).toBe(stateRank(s("Backlog", "backlog")));
    expect(stateRank(s("Done", "started"))).toBe(stateRank(s("In Progress", "started")));
    expect(stateRank(s("Duplicate", "canceled"))).toBe(stateRank(s("Canceled", "canceled")));
  });

  it("ranks an unknown open type after Triage and above closed", () => {
    const weird = s("Mystery", "something", 50);
    expect(compareStates(s("Triage", "triage", 0), weird)).toBeLessThan(0);
    expect(compareStates(weird, s("Done", "completed", -999))).toBeLessThan(0);
  });

  it("breaks equal type and position ties by state name ascending, whatever the input order", () => {
    const states = [s("Zeta", "started", 4), s("Alpha", "started", 4), s("Mid", "started", 4)];
    const expected = ["Alpha", "Mid", "Zeta"];
    expect([...states].sort(compareStates).map((x) => x.stateName)).toEqual(expected);
    expect([...states].reverse().sort(compareStates).map((x) => x.stateName)).toEqual(expected);
  });

  it("counts a missing position as 0", () => {
    const noPos = (name: string, type: string): StateLike => ({ stateName: name, stateType: type });
    expect(compareStates(s("B3", "backlog", 3), noPos("B0", "backlog"))).toBeLessThan(0);
    expect(compareStates(noPos("C0", "completed"), s("C3", "completed", 3))).toBeLessThan(0);
  });

  it("compares rank by type only", () => {
    expect(stateRank(s("Done", "completed", 3))).toBe(stateRank(s("Shipped", "completed", 954)));
    expect(stateRank(s("In Review", "started", 1002))).toBe(stateRank(s("In Progress", "started", 2)));
  });

  it("ranks a named state without a type as unknown open", () => {
    const untyped: StateLike = { stateName: "Mystery" };
    expect(compareStates(s("Triage", "triage"), untyped)).toBeLessThan(0);
    expect(compareStates(untyped, s("Done", "completed"))).toBeLessThan(0);
    expect(isClosedState(untyped)).toBe(false);
  });

  it("ranks a state with no name and no type last", () => {
    for (const st of TEAM_RR) expect(compareStates(st, {})).toBeLessThan(0);
    expect(compareStates({}, { stateName: "Mystery" })).toBeGreaterThan(0);
  });

  it("keeps an open state above a closed one whatever the positions", () => {
    expect(compareStates(s("Backlog", "backlog", -999), s("Done", "completed", 999))).toBeLessThan(0);
    expect(compareStates(s("Todo", "unstarted", 1), s("Shipped", "completed", 0))).toBeLessThan(0);
  });
});

describe("isClosedState", () => {
  it("is true exactly for completed, duplicate and canceled types", () => {
    expect(isClosedState(s("Done", "completed"))).toBe(true);
    expect(isClosedState(s("Duplicate", "duplicate"))).toBe(true);
    expect(isClosedState(s("Canceled", "canceled"))).toBe(true);
    expect(isClosedState(s("Canceled", "cancelled"))).toBe(true);
    expect(isClosedState(s("In Review", "started"))).toBe(false);
    expect(isClosedState(s("Todo", "unstarted"))).toBe(false);
    expect(isClosedState(s("Backlog", "backlog"))).toBe(false);
    expect(isClosedState(s("Triage", "triage"))).toBe(false);
  });

  it("classifies by type, not by name", () => {
    expect(isClosedState(s("Pending", "completed"))).toBe(true);
    expect(isClosedState(s("Done", "started"))).toBe(false);
    expect(isClosedState(s("Canceled", "backlog"))).toBe(false);
  });

  it("treats unknown types and missing states as open", () => {
    expect(isClosedState(s("Mystery", "something"))).toBe(false);
    expect(isClosedState({})).toBe(false);
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
      { ...s("Canceled", "canceled", 4), stateColor: "#bbb" },
      { ...s("Shipped", "completed", 954), stateColor: "#eee" },
      { ...s("In Review", "started", 1002), stateColor: "#ccc" },
      { ...s("In Progress", "started", 2), stateColor: "#fff" },
      { ...s("backlog", "backlog"), stateColor: "#ddd" },
    ];
    const opts = stateFilterOptions(issues);
    expect(opts.map((o) => o.key)).toEqual([
      "in review",
      "in progress",
      "backlog",
      "shipped",
      "canceled",
    ]);
    expect(opts[2]).toMatchObject({ name: "Backlog", stateType: "backlog", color: "#aaa" });
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

  it("orders sub-groups by type then position, closed last", () => {
    const groups = groupByState([
      issue("1", "Done", "completed", 3),
      issue("2", "Triage", "triage", 0),
      issue("3", "In Review", "started", 1002),
      issue("4", "In Progress", "started", 2),
      issue("5", "Todo", "unstarted", 1),
      issue("6", "Backlog", "backlog", 0),
      issue("7", "Shipped", "completed", 954),
      issue("8", "Canceled", "canceled", 4),
      issue("9", "Duplicate", "duplicate", 5),
    ]);
    expect(groups.map((g) => g.label)).toEqual(TEAM_RR_ORDER);
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
