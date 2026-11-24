import { describe, expect, it } from "vitest";
import { auxButtonToForward, auxReduce, releasePointerCell, reportsMouse, type AuxHeld } from "./mouseButtons";

describe("auxButtonToForward", () => {
  it("forwards middle and right buttons when the app reports mouse", () => {
    expect(auxButtonToForward(1, true)).toBe("middle");
    expect(auxButtonToForward(2, true)).toBe("right");
  });

  it("leaves the left button to the selection path", () => {
    expect(auxButtonToForward(0, true)).toBeNull();
  });

  it("forwards nothing when the app does not report mouse", () => {
    expect(auxButtonToForward(1, false)).toBeNull();
    expect(auxButtonToForward(2, false)).toBeNull();
  });

  it("ignores back and forward buttons", () => {
    expect(auxButtonToForward(3, true)).toBeNull();
    expect(auxButtonToForward(4, true)).toBeNull();
  });
});

describe("reportsMouse", () => {
  it("is true with tracking on at the live bottom without shift", () => {
    expect(reportsMouse(true, 0, false)).toBe(true);
  });

  it("is false with tracking off", () => {
    expect(reportsMouse(false, 0, false)).toBe(false);
  });

  it("is false when scrolled into history, even with tracking on", () => {
    expect(reportsMouse(true, 30, false)).toBe(false);
  });

  it("is false with tracking off and scrolled", () => {
    expect(reportsMouse(false, 30, false)).toBe(false);
  });

  it("is false with shift held, even with tracking on at the live bottom", () => {
    expect(reportsMouse(true, 0, true)).toBe(false);
  });
});

describe("auxReduce", () => {
  const right: AuxHeld = { button: 2, aux: "right", col: 4, row: 2 };
  const middle: AuxHeld = { button: 1, aux: "middle", col: 7, row: 1 };

  it("press with nothing held holds the button and sends a press at the press cell", () => {
    expect(auxReduce(null, { type: "press", press: right })).toEqual({
      held: right,
      send: { kind: "press", aux: "right", col: 4, row: 2 },
    });
  });

  it("a second press while one is held changes nothing and sends nothing", () => {
    expect(auxReduce(right, { type: "press", press: middle })).toEqual({ held: right, send: null });
  });

  it("release of the held button sends a release at the pointer cell and frees the slot", () => {
    expect(auxReduce(right, { type: "release", button: 2, cell: { col: 9, row: 5 } })).toEqual({
      held: null,
      send: { kind: "release", aux: "right", col: 9, row: 5 },
    });
  });

  it("release with no pointer cell falls back to the press cell", () => {
    expect(auxReduce(right, { type: "release", button: 2, cell: null })).toEqual({
      held: null,
      send: { kind: "release", aux: "right", col: 4, row: 2 },
    });
  });

  it("release of another button keeps the slot and sends nothing", () => {
    expect(auxReduce(right, { type: "release", button: 1, cell: { col: 9, row: 5 } })).toEqual({
      held: right,
      send: null,
    });
  });

  it("blur frees the slot and sends nothing", () => {
    expect(auxReduce(right, { type: "blur" })).toEqual({ held: null, send: null });
  });

  it("a release after blur sends nothing", () => {
    const afterBlur = auxReduce(right, { type: "blur" });
    expect(auxReduce(afterBlur.held, { type: "release", button: 2, cell: { col: 4, row: 2 } })).toEqual({
      held: null,
      send: null,
    });
  });
});

describe("releasePointerCell", () => {
  it("passes the pointer cell through at the live bottom", () => {
    expect(releasePointerCell({ col: 4, row: 2 }, 0)).toEqual({ col: 4, row: 2 });
  });

  it("drops the cell while scrolled into history so the press cell is used", () => {
    expect(releasePointerCell({ col: 4, row: 2 }, 30)).toBeNull();
  });

  it("stays null when there is no pointer cell", () => {
    expect(releasePointerCell(null, 0)).toBeNull();
  });
});
