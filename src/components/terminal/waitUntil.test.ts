import { describe, expect, it } from "vitest";
import { refitNeeded, waitUntil } from "./waitUntil";

function fakeClock(frameMs: number) {
  let t = 0;
  let frames = 0;
  return {
    deps: {
      now: () => t,
      nextFrame: () => {
        t += frameMs;
        frames += 1;
        return Promise.resolve();
      },
    },
    elapsed: () => t,
    frames: () => frames,
  };
}

describe("waitUntil", () => {
  it("resolves on the first frame where the predicate turns true", async () => {
    const clock = fakeClock(16);
    await waitUntil(() => clock.frames() >= 3, 1000, clock.deps);
    expect(clock.frames()).toBe(3);
  });

  it.each([
    ["fast frames", 16],
    ["slow frames", 300],
  ])("gives up after 1000 ms of clock time with %s", async (_name, frameMs) => {
    const clock = fakeClock(frameMs);
    await waitUntil(() => false, 1000, clock.deps);
    expect(clock.elapsed()).toBeGreaterThanOrEqual(1000);
    expect(clock.elapsed()).toBeLessThan(1000 + frameMs);
  });
});

describe("refitNeeded", () => {
  it.each([
    [false, true, true],
    [true, false, true],
    [true, true, false],
    [false, false, false],
  ])("used %s, current %s -> %s", (used, current, expected) => {
    expect(refitNeeded(used, current)).toBe(expected);
  });
});
