import type { FontMetrics } from "./fontAtlas";

export const READING_MAX_COLS = 100;

export function readingLayout(
  width: number,
  height: number,
  metrics: Pick<FontMetrics, "cssWidth" | "cssHeight">,
  region: "center" | "quake",
  enabled: boolean,
): { cols: number; rows: number; centered: boolean } {
  const rawCols = Math.floor(width / metrics.cssWidth);
  const rawRows = Math.floor(height / metrics.cssHeight);
  // Launching a shell at 1x1 is unrecoverable: many TUIs (including `claude`)
  // cache dimensions at startup and never reflow after SIGWINCH.
  const cols = rawCols >= 10 ? rawCols : 80;
  const rows = rawRows >= 3 ? rawRows : 24;
  if (enabled && region === "center" && cols > READING_MAX_COLS) {
    return { cols: READING_MAX_COLS, rows, centered: true };
  }
  return { cols, rows, centered: false };
}
