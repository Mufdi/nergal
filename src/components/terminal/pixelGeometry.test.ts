// API assumed for the Builder (src/components/terminal/pixelGeometry.ts):
//   type CellMetrics = Pick<FontMetrics, "cellWidth" | "cellHeight">   (device pixels)
//   type Sent = { cols; rows; pixelWidth; pixelHeight }
//   type ResizePayload = Sent & { sessionId: string }   (keys are the invoke payload keys)
//   pixelGeometry(cols, rows, metrics): { pixelWidth, pixelHeight }   clamped to 65535
//   nextResize(lastSent: Sent | null, sessionId, cols, rows, metrics): ResizePayload | null
// The service records `lastSent` only when the invoke resolves, so a rejection leaves it untouched.
import { describe, expect, it } from "vitest";
import { nextResize, pixelGeometry, type ResizePayload, type Sent } from "./pixelGeometry";

const cell10x20 = { cellWidth: 10, cellHeight: 20 };
const cell9x18 = { cellWidth: 9, cellHeight: 18 };

function sentOf({ cols, rows, pixelWidth, pixelHeight }: ResizePayload): Sent {
  return { cols, rows, pixelWidth, pixelHeight };
}

describe("pixelGeometry", () => {
  it("multiplies the grid by the cell and saturates at the u16 limit", () => {
    expect(pixelGeometry(80, 24, cell10x20)).toEqual({ pixelWidth: 800, pixelHeight: 480 });
    expect(pixelGeometry(9000, 24, cell10x20)).toEqual({ pixelWidth: 65535, pixelHeight: 480 });
  });
});

describe("nextResize", () => {
  it("resends the geometry when only the cell size changes", () => {
    const last: Sent = { cols: 80, rows: 24, ...pixelGeometry(80, 24, cell9x18) };

    const payload = nextResize(last, "s1", 80, 24, cell10x20);

    expect(payload).toEqual({
      sessionId: "s1",
      cols: 80,
      rows: 24,
      pixelWidth: 800,
      pixelHeight: 480,
    });
    expect(pixelGeometry(80, 24, cell10x20)).toEqual({
      pixelWidth: payload?.pixelWidth,
      pixelHeight: payload?.pixelHeight,
    });
  });

  it("sends nothing when cols, rows and geometry are unchanged", () => {
    const last: Sent = { cols: 80, rows: 24, pixelWidth: 800, pixelHeight: 480 };

    expect(nextResize(last, "s1", 80, 24, cell10x20)).toBeNull();
  });

  it("sends the same payload again after a rejected call", () => {
    let last: Sent | null = null;
    const first = nextResize(last, "s1", 80, 24, cell10x20);
    expect(first).not.toBeNull();

    // the invoke rejected, so the caller does not record it
    const retry = nextResize(last, "s1", 80, 24, cell10x20);
    expect(retry).toEqual(first);

    last = sentOf(retry as ResizePayload);
    expect(nextResize(last, "s1", 80, 24, cell10x20)).toBeNull();
  });

  it("carries the size the terminal has after the spawn, not the one it had before", () => {
    const entry = { cols: 80, rows: 24 };
    const spawned = { ...entry };
    entry.cols = 100;
    entry.rows = 30;

    const payload = nextResize(null, "s1", entry.cols, entry.rows, cell10x20);

    expect(spawned).toEqual({ cols: 80, rows: 24 });
    expect(payload).toEqual({
      sessionId: "s1",
      cols: 100,
      rows: 30,
      pixelWidth: 1000,
      pixelHeight: 600,
    });
  });

  it("applies the current geometry when re-attaching to a PTY created without pixels", () => {
    expect(nextResize(null, "s1", 80, 24, cell10x20)).toEqual({
      sessionId: "s1",
      cols: 80,
      rows: 24,
      pixelWidth: 800,
      pixelHeight: 480,
    });
  });
});
