export function clampedCell(
  x: number,
  y: number,
  cellWidth: number,
  cellHeight: number,
  cols: number,
  rows: number,
): { col: number; row: number } {
  const col = Math.min(cols - 1, Math.max(0, Math.floor(x / cellWidth)));
  const row = Math.min(rows - 1, Math.max(0, Math.floor(y / cellHeight)));
  return { col, row };
}
