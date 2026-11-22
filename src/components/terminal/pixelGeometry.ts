import type { FontMetrics } from "./fontAtlas";

export type CellMetrics = Pick<FontMetrics, "cellWidth" | "cellHeight">;

export interface Sent {
  cols: number;
  rows: number;
  pixelWidth: number;
  pixelHeight: number;
}

export interface ResizePayload extends Sent {
  sessionId: string;
}

const U16_MAX = 65535;

export function pixelGeometry(
  cols: number,
  rows: number,
  metrics: CellMetrics,
): { pixelWidth: number; pixelHeight: number } {
  return {
    pixelWidth: Math.min(cols * metrics.cellWidth, U16_MAX),
    pixelHeight: Math.min(rows * metrics.cellHeight, U16_MAX),
  };
}

export function nextResize(
  lastRequested: Sent | null,
  sessionId: string,
  cols: number,
  rows: number,
  metrics: CellMetrics,
): ResizePayload | null {
  const { pixelWidth, pixelHeight } = pixelGeometry(cols, rows, metrics);
  if (
    lastRequested &&
    lastRequested.cols === cols &&
    lastRequested.rows === rows &&
    lastRequested.pixelWidth === pixelWidth &&
    lastRequested.pixelHeight === pixelHeight
  ) {
    return null;
  }
  return { sessionId, cols, rows, pixelWidth, pixelHeight };
}

export function recordRequest(payload: ResizePayload): Sent {
  const { cols, rows, pixelWidth, pixelHeight } = payload;
  return { cols, rows, pixelWidth, pixelHeight };
}

export function afterResizeFailure(lastRequested: Sent | null, failed: Sent): Sent | null {
  if (
    lastRequested &&
    lastRequested.cols === failed.cols &&
    lastRequested.rows === failed.rows &&
    lastRequested.pixelWidth === failed.pixelWidth &&
    lastRequested.pixelHeight === failed.pixelHeight
  ) {
    return null;
  }
  return lastRequested;
}
