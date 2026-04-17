/// Canvas-based terminal renderer backed by the wezterm-term VT emulator
/// running in the Rust backend. Mirrors the ergonomics of the legacy
/// `terminalService.ts`: state lives outside React, a single host `<div>` is
/// swapped via `show/destroy`, and each session owns a canvas entry.
///
/// Flow per frame:
///   1. Backend's emitter task coalesces PTY bytes and emits `terminal:grid-update`.
///   2. We apply the changed rows into the local shadow grid.
///   3. Changed rows are redrawn on the canvas using the [`FontAtlas`].
///   4. Cursor is drawn on top (erase old cell, paint new).
///
/// No parsing happens here — all VT state lives in the backend.

import { invoke, listen } from "@/lib/tauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { CellSnapshot, GridUpdate, TerminalKeyEvent } from "@/lib/types";

import { FontAtlas, measureFont, type FontMetrics } from "./fontAtlas";
import { WEZ_FONT, WEZ_THEME, rgbaToCss } from "./theme";

// ── Types ──

interface Entry {
  sessionId: string;
  container: HTMLDivElement;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  atlas: FontAtlas;
  metrics: FontMetrics;
  cols: number;
  rows: number;
  totalRows: number;
  grid: CellSnapshot[][];
  cursor: { x: number; y: number; visible: boolean };
  title: string | null;
  unlisten: UnlistenFn;
}

// ── Module state ──

const entries = new Map<string, Entry>();
const pending = new Set<string>();
let hostElement: HTMLDivElement | null = null;
let activeId: string | null = null;

// ── Public API ──

export function setHost(el: HTMLDivElement | null): void {
  hostElement = el;
}

export async function show(
  sessionId: string,
  cwd: string,
  mode: "new" | "continue" | "resume_pick" = "new",
): Promise<void> {
  activeId = sessionId;

  for (const [id, entry] of entries) {
    entry.container.style.display = id === sessionId ? "flex" : "none";
  }

  if (entries.has(sessionId)) {
    const entry = entries.get(sessionId)!;
    requestAnimationFrame(() => {
      fit(entry);
      paintAll(entry);
      focusCanvas(entry);
    });
    return;
  }

  if (pending.has(sessionId)) return;
  if (!hostElement) return;

  pending.add(sessionId);

  try {
    const container = document.createElement("div");
    container.style.cssText =
      "position:absolute;inset:0;display:flex;overflow:hidden;background:" +
      WEZ_THEME.background +
      ";";
    hostElement.appendChild(container);

    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );

    const canvas = document.createElement("canvas");
    canvas.tabIndex = 0;
    canvas.style.cssText = "outline:none;display:block;";
    container.appendChild(canvas);

    const dpr = window.devicePixelRatio || 1;
    const metrics = measureFont(WEZ_FONT.family, WEZ_FONT.size, WEZ_FONT.lineHeight, dpr);
    const atlas = new FontAtlas(metrics, WEZ_FONT.family, WEZ_FONT.size, dpr);

    const { cols, rows } = computeCols(container, metrics);
    sizeCanvas(canvas, cols, rows, metrics);

    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("terminal canvas 2d context unavailable");

    const entry: Entry = {
      sessionId,
      container,
      canvas,
      ctx,
      atlas,
      metrics,
      cols,
      rows,
      totalRows: rows,
      grid: emptyGrid(cols, rows),
      cursor: { x: 0, y: 0, visible: true },
      title: null,
      unlisten: () => {},
    };

    wireInput(entry);

    const { pty_id } = await invoke<{ pty_id: string }>("start_claude_session", {
      sessionId,
      cwd,
      cols,
      rows,
      resume: mode === "new" ? null : mode,
    });
    void pty_id; // not used here — the session_id is what subsequent commands key off.

    entry.unlisten = await listen<GridUpdate>("terminal:grid-update", (payload) => {
      if (payload.sessionId !== sessionId) return;
      applyUpdate(entry, payload);
    });

    // Seed the shadow grid with whatever state the backend already holds
    // (for resumed sessions or reopened windows).
    try {
      const initial = await invoke<GridUpdate>("terminal_get_full_grid", { sessionId });
      applyUpdate(entry, initial);
    } catch {
      // No backend state yet — the first grid-update will paint the screen.
    }

    paintAll(entry);
    entries.set(sessionId, entry);
    focusCanvas(entry);

    if (activeId !== sessionId) {
      container.style.display = "none";
    }
  } catch (err) {
    console.error("wezterm: failed to create terminal for session", sessionId, err);
  } finally {
    pending.delete(sessionId);
  }
}

export function destroy(sessionId: string): void {
  const entry = entries.get(sessionId);
  if (!entry) return;
  entry.unlisten();
  entry.container.remove();
  entries.delete(sessionId);
  invoke("kill_session_pty", { sessionId }).catch(() => {});
}

export function fitActive(): void {
  if (!activeId) return;
  const entry = entries.get(activeId);
  if (!entry) return;
  fit(entry);
  paintAll(entry);
}

export function focusActive(): void {
  if (!activeId) return;
  const entry = entries.get(activeId);
  if (entry) focusCanvas(entry);
}

export async function writeToSession(sessionId: string, text: string): Promise<void> {
  await invoke("write_to_session_pty", { sessionId, data: text });
}

export function hasTerminal(sessionId: string): boolean {
  return entries.has(sessionId);
}

// ── Private: input wiring ──

function wireInput(entry: Entry): void {
  entry.canvas.addEventListener("keydown", (e) => {
    // Global cluihud shortcuts pass through (same list as the legacy path).
    if (shouldPassThrough(e)) return;

    e.preventDefault();

    const payload: TerminalKeyEvent = {
      code: e.code,
      key: e.key,
      text: isPrintable(e.key) ? e.key : undefined,
      ctrl: e.ctrlKey,
      shift: e.shiftKey,
      alt: e.altKey,
      meta: e.metaKey,
    };

    invoke("terminal_input", { sessionId: entry.sessionId, event: payload }).catch((err) =>
      console.error("terminal_input failed", err),
    );
  });

  entry.canvas.addEventListener("mousedown", () => focusCanvas(entry));
}

function isPrintable(key: string): boolean {
  if (key.length !== 1) return false;
  const code = key.charCodeAt(0);
  return code >= 0x20 && code !== 0x7f;
}

function shouldPassThrough(e: KeyboardEvent): boolean {
  // Same shortcut set the legacy terminalService.wireIMEFix filters — these
  // are handled by cluihud globally, not by the terminal.
  if (e.type !== "keydown") return true;
  if (e.ctrlKey && !e.shiftKey && !e.altKey) {
    if (
      e.code === "Semicolon" ||
      e.code === "KeyK" ||
      e.code === "KeyB" ||
      e.code === "KeyS" ||
      e.code === "KeyW" ||
      e.code === "KeyN" ||
      e.code === "Tab"
    ) {
      return true;
    }
    if (e.code >= "Digit1" && e.code <= "Digit9") return true;
  }
  if (e.ctrlKey && e.shiftKey && !e.altKey) {
    if (
      [
        "KeyB","KeyP","KeyF","KeyD","KeyS","KeyG","KeyK","KeyL","KeyT",
        "KeyE","KeyM","KeyN","KeyC","KeyI","KeyJ","KeyO","KeyX","KeyA",
        "KeyR","KeyU",
      ].includes(e.code)
    ) {
      return true;
    }
  }
  if (e.altKey && e.code.startsWith("Arrow")) return true;
  return false;
}

function focusCanvas(entry: Entry): void {
  entry.canvas.focus({ preventScroll: true });
}

// ── Private: resize + layout ──

function computeCols(container: HTMLElement, metrics: FontMetrics): { cols: number; rows: number } {
  const rect = container.getBoundingClientRect();
  const cols = Math.max(1, Math.floor(rect.width / metrics.cssWidth));
  const rows = Math.max(1, Math.floor(rect.height / metrics.cssHeight));
  return { cols, rows };
}

function sizeCanvas(
  canvas: HTMLCanvasElement,
  cols: number,
  rows: number,
  metrics: FontMetrics,
): void {
  canvas.width = cols * metrics.cellWidth;
  canvas.height = rows * metrics.cellHeight;
  canvas.style.width = `${cols * metrics.cssWidth}px`;
  canvas.style.height = `${rows * metrics.cssHeight}px`;
}

function fit(entry: Entry): void {
  const { cols, rows } = computeCols(entry.container, entry.metrics);
  if (cols === entry.cols && rows === entry.rows) return;
  entry.cols = cols;
  entry.rows = rows;
  entry.grid = reshapeGrid(entry.grid, cols, rows);
  sizeCanvas(entry.canvas, cols, rows, entry.metrics);
  invoke("resize_session_terminal", {
    sessionId: entry.sessionId,
    cols,
    rows,
  }).catch((err: unknown) => {
    console.error("resize_session_terminal failed", err);
  });
}

// ── Private: grid maintenance ──

function emptyCell(): CellSnapshot {
  return {
    ch: " ",
    fg: null,
    bg: null,
    bold: false,
    italic: false,
    underline: false,
    reverse: false,
    hyperlink: null,
  };
}

function emptyGrid(cols: number, rows: number): CellSnapshot[][] {
  const grid: CellSnapshot[][] = new Array(rows);
  for (let r = 0; r < rows; r += 1) {
    const row: CellSnapshot[] = new Array(cols);
    for (let c = 0; c < cols; c += 1) row[c] = emptyCell();
    grid[r] = row;
  }
  return grid;
}

function reshapeGrid(
  prev: CellSnapshot[][],
  cols: number,
  rows: number,
): CellSnapshot[][] {
  const next = emptyGrid(cols, rows);
  const copyRows = Math.min(prev.length, rows);
  for (let r = 0; r < copyRows; r += 1) {
    const copyCols = Math.min(prev[r].length, cols);
    for (let c = 0; c < copyCols; c += 1) next[r][c] = prev[r][c];
  }
  return next;
}

function applyUpdate(entry: Entry, update: GridUpdate): void {
  if (update.totalRows !== entry.totalRows) {
    entry.totalRows = update.totalRows;
    entry.grid = reshapeGrid(entry.grid, entry.cols, update.totalRows);
  }
  if (update.cols !== entry.cols) {
    // The backend's cols won the race; we rely on the next fit() cycle.
    entry.cols = update.cols;
    entry.grid = reshapeGrid(entry.grid, update.cols, entry.totalRows);
    sizeCanvas(entry.canvas, entry.cols, entry.totalRows, entry.metrics);
  }

  const previousCursor = entry.cursor;
  const touchedRows = new Set<number>();

  for (const row of update.rows) {
    if (row.index >= entry.grid.length) continue;
    entry.grid[row.index] = normalizeRow(row.cells, entry.cols);
    touchedRows.add(row.index);
  }

  entry.cursor = update.cursor;
  entry.title = update.title;

  // The cursor area needs repainting whenever the cursor moves, even if the
  // underlying row did not otherwise change.
  if (
    previousCursor.x !== update.cursor.x ||
    previousCursor.y !== update.cursor.y ||
    previousCursor.visible !== update.cursor.visible
  ) {
    touchedRows.add(previousCursor.y);
    touchedRows.add(update.cursor.y);
  }

  for (const y of touchedRows) {
    if (y >= 0 && y < entry.rows) paintRow(entry, y);
  }
  paintCursor(entry);
}

function normalizeRow(cells: CellSnapshot[], cols: number): CellSnapshot[] {
  if (cells.length === cols) return cells;
  if (cells.length > cols) return cells.slice(0, cols);
  const extended = cells.slice();
  while (extended.length < cols) extended.push(emptyCell());
  return extended;
}

// ── Private: painting ──

function paintAll(entry: Entry): void {
  entry.ctx.fillStyle = WEZ_THEME.background;
  entry.ctx.fillRect(0, 0, entry.canvas.width, entry.canvas.height);
  for (let y = 0; y < entry.rows; y += 1) paintRow(entry, y);
  paintCursor(entry);
}

function paintRow(entry: Entry, y: number): void {
  const { ctx, metrics, atlas, cols } = entry;
  const row = entry.grid[y];
  const dyCell = y * metrics.cellHeight;

  // Clear the row background first.
  ctx.fillStyle = WEZ_THEME.background;
  ctx.fillRect(0, dyCell, cols * metrics.cellWidth, metrics.cellHeight);

  for (let x = 0; x < cols; x += 1) {
    const cell = row[x];
    const dxCell = x * metrics.cellWidth;

    let fg = rgbaToCss(cell.fg, WEZ_THEME.foreground);
    let bg = rgbaToCss(cell.bg, WEZ_THEME.background);
    if (cell.reverse) {
      const swap = fg;
      fg = bg;
      bg = swap;
    }

    if (bg !== WEZ_THEME.background) {
      ctx.fillStyle = bg;
      ctx.fillRect(dxCell, dyCell, metrics.cellWidth, metrics.cellHeight);
    }

    if (cell.ch && cell.ch !== " ") {
      atlas.drawGlyph(ctx, dxCell, dyCell, cell.ch, fg, cell.bold, cell.italic);
    }

    if (cell.underline) {
      ctx.fillStyle = fg;
      ctx.fillRect(
        dxCell,
        dyCell + metrics.baseline + 1,
        metrics.cellWidth,
        Math.max(1, Math.floor(metrics.cellHeight / 16)),
      );
    }
  }
}

function paintCursor(entry: Entry): void {
  if (!entry.cursor.visible) return;
  const { ctx, metrics, cursor, cols, rows } = entry;
  if (cursor.x < 0 || cursor.x >= cols || cursor.y < 0 || cursor.y >= rows) return;

  const dx = cursor.x * metrics.cellWidth;
  const dy = cursor.y * metrics.cellHeight;

  ctx.fillStyle = WEZ_THEME.cursor;
  ctx.fillRect(dx, dy, metrics.cellWidth, metrics.cellHeight);

  const cell = entry.grid[cursor.y]?.[cursor.x];
  if (cell?.ch && cell.ch !== " ") {
    entry.atlas.drawGlyph(
      ctx,
      dx,
      dy,
      cell.ch,
      WEZ_THEME.cursorAccent,
      cell.bold,
      cell.italic,
    );
  }
}
