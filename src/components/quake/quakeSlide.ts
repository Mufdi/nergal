export const SLIDE_MS = 200;

export type SlidePhase = "hidden" | "entering" | "shown" | "exiting";

export type SlideEvent =
  | { type: "open" }
  | { type: "close"; sessionSwitch: boolean }
  | { type: "animationEnd" };

export function initialSlidePhase(open: boolean): SlidePhase {
  return open ? "shown" : "hidden";
}

export function nextSlidePhase(phase: SlidePhase, event: SlideEvent): SlidePhase {
  switch (event.type) {
    case "open":
      return phase === "hidden" || phase === "exiting" ? "entering" : phase;
    case "close":
      // A session switch must not slide out over the incoming session.
      if (event.sessionSwitch) return "hidden";
      return phase === "shown" || phase === "entering" ? "exiting" : phase;
    case "animationEnd":
      if (phase === "entering") return "shown";
      if (phase === "exiting") return "hidden";
      return phase;
  }
}

const CLASS_BY_PHASE: Record<SlidePhase, string> = {
  hidden: "quake-hidden",
  entering: "quake-enter",
  shown: "quake-shown",
  exiting: "quake-exit",
};

export function slideClass(phase: SlidePhase): string {
  return CLASS_BY_PHASE[phase];
}

export function isSliding(phase: SlidePhase): boolean {
  return phase === "entering" || phase === "exiting";
}
