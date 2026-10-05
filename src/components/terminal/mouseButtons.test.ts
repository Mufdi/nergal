import { describe, expect, it } from "vitest";
import { auxButtonToForward } from "./mouseButtons";

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
