// API assumed for the Builder (src/components/terminal/pixelGeometry.ts):
//   recordRequest(payload: ResizePayload): Sent
//     the geometry to store as lastRequested at send time (sessionId stripped).
//   afterResizeFailure(lastRequested: Sent | null, failed: Sent): Sent | null
//     null only when `failed` equals lastRequested, otherwise lastRequested is kept.
import { describe, expect, it } from "vitest";
import {
  afterResizeFailure,
  nextResize,
  recordRequest,
  type CellMetrics,
} from "./pixelGeometry";

const metrics: CellMetrics = { cellWidth: 8, cellHeight: 16 };

function request(cols: number, rows: number, lastRequested: ReturnType<typeof recordRequest> | null) {
  const payload = nextResize(lastRequested, "s1", cols, rows, metrics);
  if (!payload) throw new Error("expected a payload");
  return payload;
}

describe("resize request bookkeeping", () => {
  it("recordRequest stores the geometry without the session id", () => {
    const payload = request(100, 30, null);
    expect(recordRequest(payload)).toEqual({
      cols: 100,
      rows: 30,
      pixelWidth: 800,
      pixelHeight: 480,
    });
  });

  it("a fit that re-computes 100 while 90 is pending still gets a payload", () => {
    const first = recordRequest(request(100, 30, null));
    const second = recordRequest(request(90, 30, first));
    expect(nextResize(second, "s1", 100, 30, metrics)).not.toBeNull();
  });

  it("a rejected 100 after 90 was sent keeps 90 as the last request", () => {
    const failed = recordRequest(request(100, 30, null));
    const latest = recordRequest(request(90, 30, failed));
    const kept = afterResizeFailure(latest, failed);
    expect(kept).toEqual(latest);
    expect(nextResize(kept, "s1", 90, 30, metrics)).toBeNull();
  });

  it("a failure of the latest request clears it so the next fit retries", () => {
    const only = recordRequest(request(100, 30, null));
    const cleared = afterResizeFailure(only, only);
    expect(cleared).toBeNull();
    expect(nextResize(cleared, "s1", 100, 30, metrics)).not.toBeNull();
  });

  it("a failure with nothing recorded stays null", () => {
    const failed = recordRequest(request(100, 30, null));
    expect(afterResizeFailure(null, failed)).toBeNull();
  });
});
