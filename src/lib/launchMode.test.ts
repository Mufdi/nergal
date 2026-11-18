import { describe, expect, it } from "vitest";
import { launchAndConsume } from "./launchMode";

describe("launchAndConsume", () => {
  it("an explicit launch mode wins and leaves the fresh set intact", () => {
    const fresh = new Set(["a", "b"]);
    const result = launchAndConsume("a", { a: "continue" }, fresh);
    expect(result.mode).toBe("continue");
    expect([...result.nextFresh].sort()).toEqual(["a", "b"]);
  });

  it("a fresh session launches new and drops only its own id", () => {
    const result = launchAndConsume("a", {}, new Set(["a", "b"]));
    expect(result.mode).toBe("new");
    expect([...result.nextFresh]).toEqual(["b"]);
  });

  it("a session that is not fresh continues", () => {
    const result = launchAndConsume("a", {}, new Set(["b"]));
    expect(result.mode).toBe("continue");
    expect([...result.nextFresh]).toEqual(["b"]);
  });

  it("an explicit new mode consumes the fresh flag", () => {
    const result = launchAndConsume("a", { a: "new" }, new Set(["a"]));
    expect(result.mode).toBe("new");
    expect([...result.nextFresh]).toEqual([]);
  });

  it("never mutates the input set", () => {
    const fresh = new Set(["a", "b"]);
    const result = launchAndConsume("a", {}, fresh);
    expect([...fresh]).toEqual(["a", "b"]);
    expect(result.nextFresh).not.toBe(fresh);
  });
});
