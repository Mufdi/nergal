import { describe, expect, it } from "vitest";
import { yieldToAgent } from "@/lib/agentYield";
import { shouldPassThrough } from "./passThrough";

function key(
  code: string,
  mods: { ctrl?: boolean; shift?: boolean; alt?: boolean } = {},
  type = "keydown",
) {
  return {
    type,
    code,
    ctrlKey: !!mods.ctrl,
    shiftKey: !!mods.shift,
    altKey: !!mods.alt,
  };
}

describe("shouldPassThrough", () => {
  it("passes non-keydown events through", () => {
    expect(shouldPassThrough(key("KeyB", { ctrl: true }, "keyup"))).toBe(true);
    expect(shouldPassThrough(key("KeyG", {}, "keypress"))).toBe(true);
  });

  it("passes unmarked Ctrl+B through", () => {
    expect(shouldPassThrough(key("KeyB", { ctrl: true }))).toBe(true);
  });

  it("does not pass a yielded Ctrl+B through", () => {
    const e = key("KeyB", { ctrl: true });
    yieldToAgent(e);
    expect(shouldPassThrough(e)).toBe(false);
  });

  it("does not pass a yielded Alt+ArrowUp through", () => {
    const e = key("ArrowUp", { alt: true });
    yieldToAgent(e);
    expect(shouldPassThrough(e)).toBe(false);
  });

  it("yielding one event does not affect an identical unmarked one", () => {
    yieldToAgent(key("KeyB", { ctrl: true }));
    expect(shouldPassThrough(key("KeyB", { ctrl: true }))).toBe(true);
  });

  it("keeps Ctrl+G for the agent", () => {
    expect(shouldPassThrough(key("KeyG", { ctrl: true }))).toBe(false);
  });

  it("passes the Ctrl table through", () => {
    for (const code of ["Semicolon", "KeyK", "KeyB", "KeyS", "KeyW", "KeyN", "Tab"]) {
      expect(shouldPassThrough(key(code, { ctrl: true }))).toBe(true);
    }
    for (let i = 1; i <= 9; i++) {
      expect(shouldPassThrough(key(`Digit${i}`, { ctrl: true }))).toBe(true);
    }
  });

  it("does not pass Ctrl combos outside the table (Digit0, KeyC)", () => {
    expect(shouldPassThrough(key("Digit0", { ctrl: true }))).toBe(false);
    expect(shouldPassThrough(key("KeyC", { ctrl: true }))).toBe(false);
  });

  it("does not pass Ctrl+Alt table keys through", () => {
    expect(shouldPassThrough(key("KeyB", { ctrl: true, alt: true }))).toBe(false);
  });

  it("passes the Ctrl+Shift table through", () => {
    for (const letter of "BPFDSGKLTEMNCIJOXARU") {
      expect(shouldPassThrough(key(`Key${letter}`, { ctrl: true, shift: true }))).toBe(true);
    }
  });

  it("does not pass Ctrl+Shift+Alt or letters outside the table", () => {
    expect(shouldPassThrough(key("KeyP", { ctrl: true, shift: true, alt: true }))).toBe(false);
    expect(shouldPassThrough(key("KeyZ", { ctrl: true, shift: true }))).toBe(false);
  });

  it("passes Alt+Arrow keys through", () => {
    for (const code of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) {
      expect(shouldPassThrough(key(code, { alt: true }))).toBe(true);
    }
  });

  it("does not pass plain keys or bare arrows through", () => {
    expect(shouldPassThrough(key("KeyA"))).toBe(false);
    expect(shouldPassThrough(key("ArrowUp"))).toBe(false);
    expect(shouldPassThrough(key("Enter"))).toBe(false);
  });
});
