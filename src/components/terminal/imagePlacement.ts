import type { ImageRun } from "../../lib/types";

export interface RunRects {
  sx: number;
  sy: number;
  sw: number;
  sh: number;
  dx: number;
  dy: number;
  dw: number;
  dh: number;
}

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

export function runRects(
  run: ImageRun,
  imgW: number,
  imgH: number,
  cellW: number,
  cellH: number,
  rowY: number,
): RunRects | null {
  const u0 = clamp01(run.u0);
  const u1 = clamp01(run.u1);
  const v0 = clamp01(run.v0);
  const v1 = clamp01(run.v1);
  const sw = (u1 - u0) * imgW;
  const sh = (v1 - v0) * imgH;
  const dw = run.span * cellW - run.padLeft - run.padRight;
  const dh = cellH - run.padTop - run.padBottom;
  if (sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0) return null;
  return {
    sx: u0 * imgW,
    sy: v0 * imgH,
    sw,
    sh,
    dx: run.col * cellW + run.padLeft,
    dy: rowY + run.padTop,
    dw,
    dh,
  };
}

interface RowImagesUpdate {
  cols: number;
  totalRows: number;
  rows: { index: number; images?: ImageRun[] }[];
}

export function applyRowImages(rowImages: ImageRun[][], update: RowImagesUpdate): ImageRun[][] {
  const { cols, totalRows } = update;
  const next: ImageRun[][] = new Array(totalRows);
  for (let i = 0; i < totalRows; i += 1) {
    const prev = rowImages[i] ?? [];
    next[i] = prev.some((r) => r.col >= cols) ? prev.filter((r) => r.col < cols) : prev;
  }
  for (const row of update.rows) {
    if (row.index < 0 || row.index >= totalRows) continue;
    next[row.index] = (row.images ?? []).filter((r) => r.col < cols);
  }
  return next;
}
