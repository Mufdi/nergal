import { describe, expect, it } from "vitest";
import { cssColorToRgb } from "./cssColor";

describe("cssColorToRgb", () => {
  it.each([
    ["#fff", [255, 255, 255]],
    ["#FFFFFF", [255, 255, 255]],
    ["#18181b", [24, 24, 27]],
    ["rgb(255, 255, 255)", [255, 255, 255]],
    ["rgba(9, 9, 11, 1)", [9, 9, 11]],
    ["rgb(9,9,11)", [9, 9, 11]],
  ])("parses %s", (input, expected) => {
    expect(cssColorToRgb(input)).toEqual(expected);
  });

  it.each(["oklch(0.5 0.1 200)", "", "#ggg", "#12345", "rgb(300, 0, 0)"])(
    "rejects %j",
    (input) => {
      expect(cssColorToRgb(input)).toBeNull();
    },
  );
});
