import { describe, expect, it } from "vitest";
import { clampedCell } from "./cellMapping";

describe("clampedCell", () => {
  it.each([
    ["left of the canvas clamps to column 0", -40, 100, { col: 0, row: 6 }],
    ["right of the canvas clamps to the last column", 900, 100, { col: 99, row: 6 }],
    ["above the canvas clamps to row 0", 400, -10, { col: 50, row: 0 }],
    ["below the canvas clamps to the last row", 400, 900, { col: 50, row: 49 }],
    ["inside the canvas maps by cell size", 12, 20, { col: 1, row: 1 }],
  ])("%s", (_name, x, y, expected) => {
    expect(clampedCell(x, y, 8, 16, 100, 50)).toEqual(expected);
  });
});
