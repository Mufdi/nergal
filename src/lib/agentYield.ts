import type { FocusZone } from "@/lib/focusZoneSync";

// The dispatcher and the terminal handle the same keydown in different
// listeners; a WeakSet lets the dispatcher's decision travel with the event
// without outliving it.
const yielded = new WeakSet<object>();

export function yieldToAgent(e: object): void {
  yielded.add(e);
}

export function isYieldedToAgent(e: object): boolean {
  return yielded.has(e);
}

export function shouldYieldToAgent(o: {
  ownership: "nergal" | "agent";
  scope: "global" | "app" | undefined;
  targetZone: FocusZone | null;
}): boolean {
  return o.ownership === "agent"
    && o.scope === "app"
    && (o.targetZone === "terminal" || o.targetZone === "quake");
}
