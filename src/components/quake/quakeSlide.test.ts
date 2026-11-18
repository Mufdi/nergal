import { describe, expect, it } from "vitest";
import {
  SLIDE_MS,
  initialSlidePhase,
  isSliding,
  nextSlidePhase,
  slideClass,
  type SlidePhase,
} from "./quakeSlide";

const open = { type: "open" } as const;
const close = { type: "close" } as const;
const end = { type: "animationEnd" } as const;

describe("quake slide phases", () => {
  it("starts hidden when closed and shown when already open", () => {
    expect(initialSlidePhase(false)).toBe("hidden");
    expect(initialSlidePhase(true)).toBe("shown");
  });

  it("slides in on open and settles on animation end", () => {
    const entering = nextSlidePhase("hidden", open);
    expect(entering).toBe("entering");
    expect(nextSlidePhase(entering, end)).toBe("shown");
  });

  it("slides out on close and hides only on animation end", () => {
    const exiting = nextSlidePhase("shown", close);
    expect(exiting).toBe("exiting");
    expect(nextSlidePhase(exiting, end)).toBe("hidden");
  });

  it("closing mid-enter switches to the exit slide", () => {
    expect(nextSlidePhase("entering", close)).toBe("exiting");
  });

  it("reopening mid-exit slides back in", () => {
    expect(nextSlidePhase("exiting", open)).toBe("entering");
  });

  it("every close plays the exit slide, session switches included", () => {
    const expected: Record<SlidePhase, SlidePhase> = {
      hidden: "hidden",
      entering: "exiting",
      shown: "exiting",
      exiting: "exiting",
    };
    for (const phase of Object.keys(expected) as SlidePhase[]) {
      expect(nextSlidePhase(phase, close)).toBe(expected[phase]);
    }
  });

  it("ignores events that do not apply to the current phase", () => {
    expect(nextSlidePhase("shown", open)).toBe("shown");
    expect(nextSlidePhase("entering", open)).toBe("entering");
    expect(nextSlidePhase("hidden", close)).toBe("hidden");
    expect(nextSlidePhase("hidden", end)).toBe("hidden");
    expect(nextSlidePhase("shown", end)).toBe("shown");
  });

  it("maps each phase to its CSS class", () => {
    expect(slideClass("hidden")).toBe("quake-hidden");
    expect(slideClass("entering")).toBe("quake-enter");
    expect(slideClass("shown")).toBe("quake-shown");
    expect(slideClass("exiting")).toBe("quake-exit");
  });

  it("flags only the animated phases as sliding", () => {
    expect(isSliding("entering")).toBe(true);
    expect(isSliding("exiting")).toBe(true);
    expect(isSliding("hidden")).toBe(false);
    expect(isSliding("shown")).toBe(false);
  });

  it("keeps the slide duration in sync with the CSS keyframes", () => {
    expect(SLIDE_MS).toBe(200);
  });
});

describe("quake slide animation names", () => {
  const endNamed = (animationName: string) =>
    ({ type: "animationEnd", animationName }) as const;
  const fallback = { type: "animationEnd" } as const;

  it("ignores a late animationend from a cancelled slide", () => {
    expect(nextSlidePhase("entering", endNamed("quake-slide-up"))).toBe("entering");
    expect(nextSlidePhase("exiting", endNamed("quake-slide-down"))).toBe("exiting");
  });

  it("completes the phase when the animation name matches", () => {
    expect(nextSlidePhase("entering", endNamed("quake-slide-down"))).toBe("shown");
    expect(nextSlidePhase("exiting", endNamed("quake-slide-up"))).toBe("hidden");
  });

  it("completes unconditionally on the fallback timer event", () => {
    expect(nextSlidePhase("entering", fallback)).toBe("shown");
    expect(nextSlidePhase("exiting", fallback)).toBe("hidden");
  });
});
