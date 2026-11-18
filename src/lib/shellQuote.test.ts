import { describe, expect, it } from "vitest";
import { posixQuote } from "./shellQuote";

describe("posixQuote", () => {
  it("wraps a path with spaces in single quotes", () => {
    expect(posixQuote("/home/u/dev projects/app")).toBe("'/home/u/dev projects/app'");
  });

  it("escapes embedded single quotes", () => {
    expect(posixQuote("/it's")).toBe("'/it'\\''s'");
  });
});
