export type FocusZone = "sidebar" | "terminal" | "panel" | "quake";

// Same precedence as `detectCurrentZone` in stores/shortcuts.ts: the quake
// overlay and the right panel can sit inside another zone's DOM subtree.
const ORDER: FocusZone[] = ["quake", "panel", "sidebar", "terminal"];

export function zoneOf(el: { closest(selector: string): unknown } | null): FocusZone | null {
  if (!el) return null;
  return ORDER.find((z) => el.closest(`[data-focus-zone='${z}']`)) ?? null;
}
