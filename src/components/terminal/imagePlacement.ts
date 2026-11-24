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

export function runRects(
  run: ImageRun,
  imgW: number,
  imgH: number,
  cellW: number,
  cellH: number,
  rowY: number,
): RunRects {
  return {
    sx: run.u0 * imgW,
    sy: run.v0 * imgH,
    sw: (run.u1 - run.u0) * imgW,
    sh: (run.v1 - run.v0) * imgH,
    dx: run.col * cellW + run.padLeft,
    dy: rowY + run.padTop,
    dw: run.span * cellW - run.padLeft - run.padRight,
    dh: cellH - run.padTop - run.padBottom,
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
