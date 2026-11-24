import { describe, expect, it } from "vitest";
import { keyboardMenuKey, reportsMouse, suppressContextMenu } from "./mouseButtons";

const noMods = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };

describe("keyboardMenuKey", () => {
  it("maps Shift+F10 to shift-f10", () => {
    expect(keyboardMenuKey({ code: "F10", ...noMods, shiftKey: true })).toBe("shift-f10");
  });

  it.each(["ctrlKey", "altKey", "metaKey"] as const)("returns null for Shift+F10 with %s", (mod) => {
    expect(keyboardMenuKey({ code: "F10", ...noMods, shiftKey: true, [mod]: true })).toBeNull();
  });

  it("returns null for plain F10", () => {
    expect(keyboardMenuKey({ code: "F10", ...noMods })).toBeNull();
  });

  it("maps the ContextMenu key without modifiers to menu", () => {
    expect(keyboardMenuKey({ code: "ContextMenu", ...noMods })).toBe("menu");
  });

  it("returns null for unrelated keys", () => {
    expect(keyboardMenuKey({ code: "KeyA", ...noMods })).toBeNull();
  });
});

describe("suppressContextMenu", () => {
  it("lets the native menu open for shift-f10 even with tracking at offset 0", () => {
    expect(suppressContextMenu({ tracking: true, offset: 0, shift: false, keyboard: "shift-f10" })).toBe(false);
  });

  it("suppresses the menu key when tracking at offset 0", () => {
    expect(suppressContextMenu({ tracking: true, offset: 0, shift: false, keyboard: "menu" })).toBe(true);
  });

  it("does not suppress the menu key without tracking", () => {
    expect(suppressContextMenu({ tracking: false, offset: 0, shift: false, keyboard: "menu" })).toBe(false);
  });

  it("does not suppress the menu key when scrolled back", () => {
    expect(suppressContextMenu({ tracking: true, offset: 5, shift: false, keyboard: "menu" })).toBe(false);
  });

  describe("pointer path (keyboard null) follows reportsMouse", () => {
    const table = [true, false].flatMap((tracking) =>
      [0, 3].flatMap((offset) => [true, false].map((shift) => ({ tracking, offset, shift }))),
    );

    it.each(table)("tracking=$tracking offset=$offset shift=$shift", ({ tracking, offset, shift }) => {
      expect(suppressContextMenu({ tracking, offset, shift, keyboard: null })).toBe(
        reportsMouse(tracking, offset, shift),
      );
    });
  });
});
