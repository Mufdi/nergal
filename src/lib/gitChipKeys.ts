// The chip listeners sit on `window`, so without the zone check the sidebar's
// arrow keys also move the chip cursors.
export function chipKeysActive(o: {
  inZen: boolean;
  anyZenOpen: boolean;
  zenZone: string;
  focusZone: "sidebar" | "terminal" | "panel" | "quake";
}): boolean {
  if (o.inZen) return o.anyZenOpen && o.zenZone === "sidebar";
  return !o.anyZenOpen && o.focusZone === "panel";
}
