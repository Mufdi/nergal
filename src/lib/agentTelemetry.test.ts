import { describe, expect, it } from "vitest";
import type { ActivityEntry } from "./types";
import {
  applySubagentStart,
  handbackReport,
  closeUnclaimedSubagentToolRows,
  applySubagentStop,
  appendEntryPinned,
  buildToolUseEntry,
  closeRunningMainToolRows,
  formatTokens,
  SUBAGENT_NO_REPORT,
  effortChipText,
  formatMissCauses,
  notificationAttentionMessage,
  pickToolRowToComplete,
  promptCacheChip,
  shouldClearOnBackgroundCompletion,
  shouldClearOnTurnEvent,
  shouldLogGenericActivity,
  type PromptCacheStatus,
} from "./agentTelemetry";

function tool(
  id: string,
  toolName: string,
  overrides: Partial<ActivityEntry> = {},
): ActivityEntry {
  return {
    id,
    timestamp: 1000,
    type: "tool_use",
    message: toolName,
    toolName,
    status: "running",
    ...overrides,
  };
}

function pc(overrides: Partial<PromptCacheStatus> = {}): PromptCacheStatus {
  return {
    warm: null,
    caching_observed: null,
    ttl: null,
    expires_at: null,
    requests: null,
    misses: null,
    hit_ratio: null,
    miss_recache_tokens: null,
    last_miss_at: null,
    last_miss_causes: [],
    ...overrides,
  };
}

describe("notificationAttentionMessage", () => {
  it("words a background agent input request", () => {
    expect(notificationAttentionMessage("agent_needs_input")).toBe(
      "Background agent needs input",
    );
  });

  it("keeps the generic wording for other types", () => {
    expect(notificationAttentionMessage("permission_prompt")).toBe(
      "Claude needs attention: permission_prompt",
    );
  });

  it("falls back to input when the type is missing", () => {
    expect(notificationAttentionMessage(undefined)).toBe("Claude needs attention: input");
    expect(notificationAttentionMessage(null)).toBe("Claude needs attention: input");
  });
});

describe("shouldClearOnBackgroundCompletion", () => {
  it("is true only for agent_needs_input", () => {
    expect(shouldClearOnBackgroundCompletion("agent_needs_input")).toBe(true);
    expect(shouldClearOnBackgroundCompletion("permission_prompt")).toBe(false);
    expect(shouldClearOnBackgroundCompletion("agent_completed")).toBe(false);
    expect(shouldClearOnBackgroundCompletion(undefined)).toBe(false);
  });
});

describe("pickToolRowToComplete", () => {
  it("returns the last running row with the same tool name", () => {
    const list = [tool("a", "Read"), tool("b", "Bash"), tool("c", "Read")];
    expect(pickToolRowToComplete(list, "Read")).toBe(2);
  });

  it("skips done rows of the same name", () => {
    const list = [tool("a", "Read"), tool("b", "Read", { status: "done" })];
    expect(pickToolRowToComplete(list, "Read")).toBe(0);
  });

  it("falls back to the last running row that is not Agent or Task", () => {
    const list = [
      tool("a", "Bash"),
      tool("b", "Agent"),
      tool("c", "Task"),
    ];
    expect(pickToolRowToComplete(list, "Edit")).toBe(0);
  });

  it("falls back when no tool name is given", () => {
    const list = [tool("a", "Read"), tool("b", "Agent")];
    expect(pickToolRowToComplete(list)).toBe(0);
  });

  it("returns -1 when only Agent and Task rows are running", () => {
    const list = [tool("a", "Agent"), tool("b", "Task")];
    expect(pickToolRowToComplete(list, "Edit")).toBe(-1);
  });

  it("matches an Agent row when its own name is requested", () => {
    const list = [tool("a", "Read"), tool("b", "Agent")];
    expect(pickToolRowToComplete(list, "Agent")).toBe(1);
  });

  it("returns -1 for an empty list", () => {
    expect(pickToolRowToComplete([], "Read")).toBe(-1);
  });

  it("ignores non tool_use entries", () => {
    const list: ActivityEntry[] = [
      { id: "s", timestamp: 1, type: "session", message: "x", status: "running" },
    ];
    expect(pickToolRowToComplete(list, "Read")).toBe(-1);
  });
});

describe("applySubagentStart", () => {
  const p = { agentId: "a1", agentType: "Explore", id: "new", timestamp: 2000 };

  it("claims the newest unclaimed running Agent row", () => {
    const list = [
      tool("old", "Agent", { timestamp: 100 }),
      tool("newer", "Agent", { timestamp: 200 }),
    ];
    const out = applySubagentStart(list, p);
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual(list[0]);
    expect(out[1]).toEqual({
      ...list[1],
      type: "subagent",
      agentId: "a1",
      message: "Subagent: Explore",
    });
  });

  it("keeps the claimed row's id, timestamp and status", () => {
    const out = applySubagentStart([tool("row", "Agent", { timestamp: 555 })], p);
    expect(out[0]).toMatchObject({ id: "row", timestamp: 555, status: "running" });
  });

  it("claims a Task row too", () => {
    const out = applySubagentStart([tool("t", "Task")], p);
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ id: "t", type: "subagent", agentId: "a1" });
  });

  it("appends a running subagent row when no Agent row exists", () => {
    const list = [tool("r", "Read")];
    const out = applySubagentStart(list, p);
    expect(out).toHaveLength(2);
    expect(out[1]).toEqual({
      id: "new",
      timestamp: 2000,
      type: "subagent",
      message: "Subagent: Explore",
      status: "running",
      agentId: "a1",
    });
  });

  it("ignores an Agent row already claimed by another subagent", () => {
    const claimed = tool("c", "Agent", { agentId: "a0" });
    const out = applySubagentStart([claimed], p);
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual(claimed);
    expect(out[1]).toMatchObject({ id: "new", agentId: "a1" });
  });

  it("claims the unclaimed row when a claimed one is newer", () => {
    const older = tool("free", "Agent", { timestamp: 100 });
    const newer = tool("taken", "Agent", { timestamp: 200, agentId: "a0" });
    const out = applySubagentStart([older, newer], p);
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual(newer);
    expect(out[1]).toMatchObject({ id: "free", type: "subagent", agentId: "a1" });
  });

  it("ignores a done Agent row", () => {
    const done = tool("d", "Agent", { status: "done" });
    const out = applySubagentStart([done], p);
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual(done);
    expect(out[1]).toMatchObject({ id: "new", type: "subagent" });
  });

  it("does not mutate the input list", () => {
    const list = [tool("a", "Agent")];
    const snapshot = structuredClone(list);
    const out = applySubagentStart(list, p);
    expect(list).toEqual(snapshot);
    expect(out).not.toBe(list);
  });
});

describe("applySubagentStop", () => {
  function running(id: string, agentId: string, timestamp = 1000): ActivityEntry {
    return {
      id,
      timestamp,
      type: "subagent",
      message: "Subagent: Explore",
      status: "running",
      agentId,
    };
  }

  const base = { agentId: "a1", agentType: "Explore", id: "stop", timestamp: 4000 };

  it("completes the running row with the same agentId", () => {
    const out = applySubagentStop([running("r", "a1", 1000)], base);
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({
      id: "r",
      type: "subagent",
      message: "Subagent: Explore",
      status: "done",
      durationMs: 3000,
      agentId: "a1",
    });
  });

  it("sets detail from the last assistant message", () => {
    const out = applySubagentStop([running("r", "a1")], { ...base, message: "found 3 files" });
    expect(out[0].detail).toBe("found 3 files");
  });

  it("keeps the existing detail when no message is given", () => {
    const row = { ...running("r", "a1"), detail: "earlier" };
    const out = applySubagentStop([row], base);
    expect(out[0].detail).toBe("earlier");
  });

  it("appends the effort to the label", () => {
    const out = applySubagentStop([running("r", "a1")], { ...base, effort: "high" });
    expect(out[0].message).toBe("Subagent: Explore · high");
  });

  it("completes only the matching agent among concurrent ones", () => {
    const list = [running("r1", "a1", 1000), running("r2", "a2", 1500)];
    const out = applySubagentStop(list, { ...base, agentId: "a2", timestamp: 3500 });
    expect(out[0]).toMatchObject({ id: "r2", status: "done", durationMs: 2000 });
    expect(out[1]).toEqual(list[0]);
  });

  it("appends a standalone done row when no running row matches", () => {
    const out = applySubagentStop([running("r", "other")], {
      ...base,
      message: "all done",
      effort: "low",
    });
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual({
      id: "stop",
      timestamp: 4000,
      type: "subagent",
      message: "Subagent: Explore · low",
      status: "done",
      agentId: "a1",
      detail: "all done",
    });
  });

  it("appends a standalone row without detail when no message is given", () => {
    const out = applySubagentStop([], base);
    expect(out).toHaveLength(1);
    expect(out[0].detail).toBe(SUBAGENT_NO_REPORT);
    expect(out[0]).toMatchObject({ status: "done", message: "Subagent: Explore" });
  });

  it("does not reuse an already completed row for the same agentId", () => {
    const done: ActivityEntry = { ...running("r", "a1"), status: "done" };
    const out = applySubagentStop([done], base);
    expect(out).toHaveLength(2);
    expect(out[0]).toEqual(done);
  });

  it("does not mutate the input list", () => {
    const list = [running("r", "a1")];
    const snapshot = structuredClone(list);
    applySubagentStop(list, base);
    expect(list).toEqual(snapshot);
  });
});

describe("applySubagentStop with agent-tagged rows", () => {
  const sub = (id: string, agentId: string, timestamp: number): ActivityEntry => ({
    id,
    timestamp,
    type: "subagent",
    message: "Subagent: Explore",
    status: "running",
    agentId,
  });
  const base = { agentId: "a1", agentType: "Explore", id: "stop", timestamp: 9000 };

  it("closes running rows owned by the stopping agent only", () => {
    const list = [
      sub("s1", "a1", 1000),
      sub("s2", "a2", 1100),
      tool("t1", "Bash", { timestamp: 2000, ownerAgentId: "a1" }),
      tool("t2", "Bash", { timestamp: 2100, ownerAgentId: "a2" }),
      tool("t3", "Bash", { timestamp: 2200 }),
    ];
    const out = applySubagentStop(list, base);
    const byId = Object.fromEntries(out.map((e) => [e.id, e]));
    expect(byId.t1).toMatchObject({ status: "done", durationMs: 7000 });
    expect(byId.t2.status).toBe("running");
    expect(byId.t3.status).toBe("running");
    expect(byId.s2.status).toBe("running");
  });

  it("leaves already done owned rows untouched", () => {
    const done = tool("t1", "Bash", { ownerAgentId: "a1", status: "done", durationMs: 5 });
    const out = applySubagentStop([sub("s1", "a1", 1000), done], base);
    expect(out.find((e) => e.id === "t1")).toEqual(done);
  });

  it("uses the handback message as the subagent detail", () => {
    const list = [
      sub("s1", "a1", 1000),
      tool("hb", "SubagentHandback", { ownerAgentId: "a1", detail: "final report" }),
    ];
    const out = applySubagentStop(list, { ...base, message: "" });
    expect(out.find((e) => e.id === "s1")?.detail).toBe("final report");
  });

  it("prefers the handback report over last_assistant_message", () => {
    const list = [
      sub("s1", "a1", 1000),
      tool("hb", "SubagentHandback", { ownerAgentId: "a1", detail: "report" }),
    ];
    const out = applySubagentStop(list, { ...base, message: "chatter" });
    expect(out.find((e) => e.id === "s1")?.detail).toBe("report");
  });

  it("ignores another agent's handback and an empty last message", () => {
    const list = [
      sub("s1", "a1", 1000),
      tool("hb", "SubagentHandback", { ownerAgentId: "a2", detail: "other" }),
    ];
    const out = applySubagentStop(list, { ...base, message: "" });
    expect(out.find((e) => e.id === "s1")?.detail).toBe(SUBAGENT_NO_REPORT);
  });

  it("falls back to last_assistant_message when there is no handback", () => {
    const out = applySubagentStop([sub("s1", "a1", 1000)], { ...base, message: "found it" });
    expect(out[0].detail).toBe("found it");
  });

  it("moves the stopped row to the head with the stop time and keeps the duration", () => {
    const list = [
      sub("s1", "a1", 1000),
      tool("t1", "Bash", { timestamp: 2000, status: "done" }),
      tool("t2", "Read", { timestamp: 3000, status: "done" }),
    ];
    const out = applySubagentStop(list, base);
    expect(out.map((e) => e.id)).toEqual(["t1", "t2", "s1"]);
    expect(out[2]).toMatchObject({ timestamp: 9000, durationMs: 8000, status: "done" });
  });

  it("still closes unclaimed Agent/Task rows regardless of owner", () => {
    const list = [sub("s1", "a1", 1000), tool("n", "Agent", { ownerAgentId: "a1" })];
    expect(closeUnclaimedSubagentToolRows(list, 5000)[1].status).toBe("done");
  });
});

describe("handbackReport", () => {
  it("reads tool_input.message from a SubagentHandback call", () => {
    expect(handbackReport("SubagentHandback", { message: "done" })).toBe("done");
  });

  it("returns undefined for other tools, missing or blank messages", () => {
    expect(handbackReport("Bash", { message: "x" })).toBeUndefined();
    expect(handbackReport("SubagentHandback", {})).toBeUndefined();
    expect(handbackReport("SubagentHandback", { message: "  " })).toBeUndefined();
    expect(handbackReport("SubagentHandback", null)).toBeUndefined();
  });
});

describe("formatMissCauses", () => {
  it.each([
    ["tools_changed", "tool definitions changed"],
    ["system_prompt_changed", "system prompt changed"],
    ["ttl_expired_5m", "idle past the 5m TTL"],
    ["ttl_expired_1h", "idle past the 1h TTL"],
    ["likely_server_side", "likely server-side eviction"],
  ])("words %s", (id, expected) => {
    expect(formatMissCauses([id])).toBe(expected);
  });

  it("replaces underscores for an unknown id", () => {
    expect(formatMissCauses(["some_new_cause"])).toBe("some new cause");
  });

  it("joins several causes with a comma", () => {
    expect(formatMissCauses(["tools_changed", "system_prompt_changed"])).toBe(
      "tool definitions changed, system prompt changed",
    );
  });

  it("mixes known and unknown ids", () => {
    expect(formatMissCauses(["ttl_expired_1h", "some_new_cause"])).toBe(
      "idle past the 1h TTL, some new cause",
    );
  });

  it("returns an empty string for no causes", () => {
    expect(formatMissCauses([])).toBe("");
  });
});

describe("promptCacheChip", () => {
  it("shows the rounded percentage for a warm cache", () => {
    expect(promptCacheChip(pc({ hit_ratio: 0.91, warm: true, caching_observed: true }))).toBe("91%");
  });

  it("appends cold when the cache is not warm", () => {
    expect(promptCacheChip(pc({ hit_ratio: 0.42, warm: false }))).toBe("42% cold");
  });

  it("rounds to the nearest percent", () => {
    expect(promptCacheChip(pc({ hit_ratio: 0.906 }))).toBe("91%");
  });

  it("shows no cold marker when warm is unknown", () => {
    expect(promptCacheChip(pc({ hit_ratio: 0.5, warm: null }))).toBe("50%");
  });

  it("hides the chip when caching was not observed", () => {
    expect(promptCacheChip(pc({ caching_observed: false, hit_ratio: null }))).toBeNull();
    expect(promptCacheChip(pc({ caching_observed: false, hit_ratio: 0.9 }))).toBeNull();
  });

  it("hides the chip when the ratio is null", () => {
    expect(promptCacheChip(pc({ hit_ratio: null, warm: true }))).toBeNull();
  });

  it("hides the chip for nullish input", () => {
    expect(promptCacheChip(null)).toBeNull();
    expect(promptCacheChip(undefined)).toBeNull();
  });
});

describe("effortChipText", () => {
  it("shows one value when setting and last response match", () => {
    expect(effortChipText("high", "high")).toBe("high");
  });

  it("shows both when they differ", () => {
    expect(effortChipText("high", "medium")).toBe("high ran medium");
  });

  it("shows the setting alone", () => {
    expect(effortChipText("low", null)).toBe("low");
    expect(effortChipText("low", undefined)).toBe("low");
  });

  it("shows the last response alone when no setting is known", () => {
    expect(effortChipText(null, "xhigh")).toBe("xhigh");
    expect(effortChipText(undefined, "xhigh")).toBe("xhigh");
  });

  it("is null when neither is known", () => {
    expect(effortChipText(null, null)).toBeNull();
    expect(effortChipText(undefined, undefined)).toBeNull();
  });
});

describe("closeUnclaimedSubagentToolRows", () => {
  const row = (over: Partial<ActivityEntry>): ActivityEntry => ({
    id: Math.random().toString(36),
    timestamp: 1_000,
    type: "tool_use",
    message: "Agent",
    status: "running",
    toolName: "Agent",
    ...over,
  });

  it("closes running Agent and Task rows that no subagent claimed", () => {
    const list = [row({}), row({ toolName: "Task", message: "Task" })];
    const out = closeUnclaimedSubagentToolRows(list, 4_000);
    expect(out.map((e) => e.status)).toEqual(["done", "done"]);
    expect(out[0].durationMs).toBe(3_000);
  });

  it("leaves claimed subagent rows and other running tools alone", () => {
    const list = [
      row({ type: "subagent", agentId: "a1", message: "Subagent: Explore" }),
      row({ toolName: "Bash", message: "Bash" }),
    ];
    const out = closeUnclaimedSubagentToolRows(list, 4_000);
    expect(out[0].status).toBe("running");
    expect(out[1].status).toBe("running");
  });

  it("returns the same array when nothing changes", () => {
    const list = [row({ status: "done" })];
    expect(closeUnclaimedSubagentToolRows(list, 4_000)).toBe(list);
  });
});

describe("shouldClearOnTurnEvent", () => {
  it("keeps a background agent input wait across foreground turn events", () => {
    expect(shouldClearOnTurnEvent("agent_needs_input")).toBe(false);
  });

  it("clears every other attention source", () => {
    expect(shouldClearOnTurnEvent("permission_prompt")).toBe(true);
    expect(shouldClearOnTurnEvent("input")).toBe(true);
    expect(shouldClearOnTurnEvent(undefined)).toBe(true);
  });
});

describe("shouldLogGenericActivity", () => {
  it("skips status snapshots that fire on every render", () => {
    expect(shouldLogGenericActivity("agent_status")).toBe(false);
    expect(shouldLogGenericActivity("statusline")).toBe(false);
  });

  it("keeps logging unknown event types", () => {
    expect(shouldLogGenericActivity("config_change")).toBe(true);
  });
});

describe("pinned running subagents", () => {
  const subRow = (id: string, agentId: string, timestamp: number, status: "running" | "done" = "running"): ActivityEntry => ({
    id,
    timestamp,
    type: "subagent",
    message: "Subagent: Explore",
    status,
    agentId,
  });

  it("start moves the claimed Agent row to the head of the list", () => {
    const list = [tool("agent", "Agent", { timestamp: 100 }), tool("r", "Read", { timestamp: 200, status: "done" })];
    const out = applySubagentStart(list, { agentId: "a1", agentType: "Explore", id: "new", timestamp: 300 });
    expect(out.map((e) => e.id)).toEqual(["r", "agent"]);
    expect(out[1]).toMatchObject({ type: "subagent", timestamp: 100 });
  });

  it("newer rows land below a running subagent", () => {
    const out = appendEntryPinned([subRow("s", "a1", 100)], tool("t", "Read", { timestamp: 200 }));
    expect(out.map((e) => e.id)).toEqual(["t", "s"]);
  });

  it("keeps several running subagents in their relative order", () => {
    const list = [subRow("s1", "a1", 100), subRow("s2", "a2", 110)];
    const out = appendEntryPinned(list, tool("t", "Read"));
    expect(out.map((e) => e.id)).toEqual(["t", "s1", "s2"]);
  });

  it("appends normally when no subagent is running", () => {
    const list = [subRow("s", "a1", 100, "done")];
    expect(appendEntryPinned(list, tool("t", "Read")).map((e) => e.id)).toEqual(["s", "t"]);
  });

  it("does not mutate the input list", () => {
    const list = [subRow("s", "a1", 100)];
    appendEntryPinned(list, tool("t", "Read"));
    expect(list.map((e) => e.id)).toEqual(["s"]);
  });

  it("on stop the row settles at the stop time below subagents still running", () => {
    const list = [tool("t", "Read", { status: "done" }), subRow("s2", "a2", 150), subRow("s1", "a1", 100)];
    const out = applySubagentStop(list, { agentId: "a1", agentType: "Explore", id: "stop", timestamp: 900 });
    expect(out.map((e) => e.id)).toEqual(["t", "s1", "s2"]);
    expect(out[1]).toMatchObject({ status: "done", timestamp: 900, durationMs: 800 });
  });
});

describe("buildToolUseEntry", () => {
  const base = { id: "x", timestamp: 5, toolName: "Read" };

  it("labels a subagent's tool row with its agent type", () => {
    const e = buildToolUseEntry({ ...base, agentId: "a1", agentType: "reader-1" });
    expect(e).toMatchObject({ type: "tool_use", status: "running", ownerAgentId: "a1", ownerLabel: "reader-1" });
  });

  it("leaves main-agent rows unlabelled", () => {
    const e = buildToolUseEntry(base);
    expect(e.ownerAgentId).toBeUndefined();
    expect(e.ownerLabel).toBeUndefined();
  });

  it("falls back to no label when the agent type is empty", () => {
    expect(buildToolUseEntry({ ...base, agentId: "a1", agentType: "" }).ownerLabel).toBeUndefined();
  });
});

describe("applySubagentStop detail and tokens", () => {
  const sub: ActivityEntry = {
    id: "s",
    timestamp: 1000,
    type: "subagent",
    message: "Subagent: Explore",
    status: "running",
    agentId: "a1",
  };
  const base = { agentId: "a1", agentType: "Explore", id: "stop", timestamp: 4000 };

  it("uses the stopped-before-reporting fallback when nothing reported", () => {
    expect(applySubagentStop([sub], base)[0].detail).toBe("Stopped before reporting");
    expect(applySubagentStop([sub], { ...base, message: "" })[0].detail).toBe("Stopped before reporting");
  });

  it("prefers the message, then the handback, over the fallback", () => {
    expect(applySubagentStop([sub], { ...base, message: "text" })[0].detail).toBe("text");
  });

  it("stores the token count on the settled row", () => {
    expect(applySubagentStop([sub], { ...base, tokens: 48_500 })[0].tokens).toBe(48_500);
  });

  it("stores tokens on a standalone stop row too", () => {
    expect(applySubagentStop([], { ...base, tokens: 900 })[0].tokens).toBe(900);
  });

  it("leaves tokens undefined when the backend sent none", () => {
    expect(applySubagentStop([sub], base)[0].tokens).toBeUndefined();
  });
});

describe("formatTokens", () => {
  it("matches the status bar format", () => {
    expect(formatTokens(48_500)).toBe("48.5K");
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(120_000)).toBe("120K");
  });
});

describe("closeRunningMainToolRows", () => {
  it("closes running main-agent rows without inventing a duration", () => {
    const list = [tool("ts", "ToolSearch"), tool("stop", "TaskStop")];
    const out = closeRunningMainToolRows(list);
    expect(out.every((e) => e.status === "done")).toBe(true);
    expect(out.every((e) => e.durationMs === undefined)).toBe(true);
  });

  it("leaves subagent-owned rows and already done rows alone", () => {
    const owned = tool("o", "Bash", { ownerAgentId: "a1" });
    const done = tool("d", "Read", { status: "done", durationMs: 12 });
    const out = closeRunningMainToolRows([owned, done]);
    expect(out[0]).toEqual(owned);
    expect(out[1]).toEqual(done);
  });

  it("ignores non tool rows and returns the same list when nothing changes", () => {
    const list: ActivityEntry[] = [{ id: "s", timestamp: 1, type: "session", message: "x" }];
    expect(closeRunningMainToolRows(list)).toBe(list);
  });
});
