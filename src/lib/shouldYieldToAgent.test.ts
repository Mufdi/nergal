import { describe, it, expect } from "vitest";
import { shouldYieldToAgent } from "@/lib/agentYield";

type Zone = "sidebar" | "terminal" | "panel" | "quake" | null;
const scopes = ["global", "app", undefined] as const;
const zones: Zone[] = ["sidebar", "terminal", "panel", "quake", null];

describe("shouldYieldToAgent", () => {
  it.each(scopes.flatMap((scope) => zones.map((targetZone) => ({ scope, targetZone }))))(
    "nergal ownership never yields (scope=$scope, zone=$targetZone)",
    ({ scope, targetZone }) => {
      expect(shouldYieldToAgent({ ownership: "nergal", scope, targetZone })).toBe(false);
    },
  );

  it.each(zones.map((targetZone) => ({ targetZone })))(
    "agent ownership with global scope never yields (zone=$targetZone)",
    ({ targetZone }) => {
      expect(shouldYieldToAgent({ ownership: "agent", scope: "global", targetZone })).toBe(false);
    },
  );

  it.each(zones.map((targetZone) => ({ targetZone })))(
    "agent ownership with undefined scope never yields (zone=$targetZone)",
    ({ targetZone }) => {
      expect(shouldYieldToAgent({ ownership: "agent", scope: undefined, targetZone })).toBe(false);
    },
  );

  it.each(["terminal", "quake"] as const)("agent + app scope yields in %s", (targetZone) => {
    expect(shouldYieldToAgent({ ownership: "agent", scope: "app", targetZone })).toBe(true);
  });

  it.each(["sidebar", "panel", null] as const)("agent + app scope does not yield in %s", (targetZone) => {
    expect(shouldYieldToAgent({ ownership: "agent", scope: "app", targetZone })).toBe(false);
  });
});
