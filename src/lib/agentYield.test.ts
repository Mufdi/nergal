import { describe, expect, it } from "vitest";
import { isYieldedToAgent, yieldToAgent } from "./agentYield";

describe("agentYield", () => {
  it("is false for an unmarked event", () => {
    expect(isYieldedToAgent({ code: "KeyB" })).toBe(false);
  });

  it("is true after yieldToAgent marks the event", () => {
    const e = { code: "KeyB" };
    yieldToAgent(e);
    expect(isYieldedToAgent(e)).toBe(true);
  });

  it("marks only the given event object", () => {
    const marked = { code: "KeyB" };
    const other = { code: "KeyB" };
    yieldToAgent(marked);
    expect(isYieldedToAgent(marked)).toBe(true);
    expect(isYieldedToAgent(other)).toBe(false);
  });

  it("is idempotent when marked twice", () => {
    const e = {};
    yieldToAgent(e);
    yieldToAgent(e);
    expect(isYieldedToAgent(e)).toBe(true);
  });
});
