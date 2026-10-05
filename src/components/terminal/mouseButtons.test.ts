import { describe, expect, it } from "vitest";
import { auxButtonToForward, auxPress, auxRelease, type AuxHeld } from "./mouseButtons";

describe("auxButtonToForward", () => {
  it("forwards middle and right buttons over the alt screen", () => {
    expect(auxButtonToForward(1, true)).toBe("middle");
    expect(auxButtonToForward(2, true)).toBe("right");
  });

  it("leaves the left button to the selection path", () => {
    expect(auxButtonToForward(0, true)).toBeNull();
  });

  it("forwards nothing on the primary screen", () => {
    expect(auxButtonToForward(1, false)).toBeNull();
    expect(auxButtonToForward(2, false)).toBeNull();
  });

  it("ignores back and forward buttons", () => {
    expect(auxButtonToForward(3, true)).toBeNull();
    expect(auxButtonToForward(4, true)).toBeNull();
  });
});

describe("aux held slot", () => {
  const right: AuxHeld = { button: 2, aux: "right", col: 4, row: 2 };
  const middle: AuxHeld = { button: 1, aux: "middle", col: 7, row: 1 };

  it("press then release sends the release at the press cell and frees the slot", () => {
    const pressed = auxPress(null, right);
    expect(pressed).toEqual({ held: right, sendPress: true });
    const released = auxRelease(pressed.held, 2);
    expect(released).toEqual({ held: null, release: right });
  });

  it("press, blur, release sends no release", () => {
    const pressed = auxPress(null, right);
    const afterBlur: AuxHeld | null = null;
    expect(pressed.sendPress).toBe(true);
    expect(auxRelease(afterBlur, 2)).toEqual({ held: null, release: null });
  });

  it("ignores a second aux press while one is held", () => {
    const first = auxPress(null, right);
    const second = auxPress(first.held, middle);
    expect(second).toEqual({ held: right, sendPress: false });
  });

  it("a release of another button keeps the slot", () => {
    expect(auxRelease(right, 1)).toEqual({ held: right, release: null });
  });

  it("a release with nothing held sends nothing", () => {
    expect(auxRelease(null, 2)).toEqual({ held: null, release: null });
  });
});
