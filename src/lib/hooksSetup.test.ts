import { describe, it, expect } from "vitest";
import { shouldOpenHooksDialog, canConfigure } from "@/lib/hooksSetup";
import type { HooksStatus } from "@/lib/types";

function status(overrides: Partial<HooksStatus>): HooksStatus {
  return {
    registered: false,
    settings_path: "/home/u/.claude/settings.json",
    binary_on_path: true,
    ...overrides,
  };
}

describe("shouldOpenHooksDialog", () => {
  it("is false when hooks are already registered", () => {
    expect(shouldOpenHooksDialog(status({ registered: true }))).toBe(false);
  });

  it("is true when hooks are not registered", () => {
    expect(shouldOpenHooksDialog(status({ registered: false }))).toBe(true);
  });

  it("is false while the status has not loaded", () => {
    expect(shouldOpenHooksDialog(null)).toBe(false);
  });
});

describe("canConfigure", () => {
  it("is true when the binary is on PATH", () => {
    expect(canConfigure(status({ binary_on_path: true }))).toBe(true);
  });

  it("is false when the binary is not on PATH", () => {
    expect(canConfigure(status({ binary_on_path: false }))).toBe(false);
  });
});
