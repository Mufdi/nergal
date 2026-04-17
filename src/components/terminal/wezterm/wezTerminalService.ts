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
import { open as openShell } from "@tauri-apps/plugin-shell";
import {
  readText as readClipboard,
  writeText as writeClipboard,
} from "@tauri-apps/plugin-clipboard-manager";
import type { CellSnapshot, GridUpdate, TerminalKeyEvent } from "@/lib/types";

import { FontAtlas, measureFont, type FontMetrics } from "./fontAtlas";
import { WEZ_FONT, WEZ_THEME, rgbaToCss } from "./theme";

interface CellCoord {
  col: number;
  row: number;
}

interface Selection {
  anchor: CellCoord;
  head: CellCoord;
}

// ── Types ──

interface Entry {
  sessionId: string;
  container: HTMLDivElement;
  canvas: HTMLCanvasElement;
  /// Hidden 1x1 textarea overlay used exclusively as the keyboard focus
  /// target so the browser gives us proper `compositionstart/end` events
  /// for dead keys, IME, and accented-character composition. The canvas
  /// itself only handles mouse events.
  textarea: HTMLTextAreaElement;
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
  selection: Selection | null;
  isDragging: boolean;
  hoveredHyperlink: string | null;
  composing: boolean;
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
    canvas.style.cssText = "outline:none;display:block;";
    container.appendChild(canvas);

    // Hidden focus target. 1×1 px positioned at origin keeps it unobtrusive
    // but still a first-class input surface so IME popovers (if the OS
    // shows them) appear near the top-left instead of floating offscreen.
    const textarea = document.createElement("textarea");
    textarea.setAttribute("aria-label", "Terminal input");
    textarea.autocapitalize = "off";
    textarea.autocomplete = "off";
    textarea.spellcheck = false;
    textarea.style.cssText = [
      "position:absolute",
      "top:0",
      "left:0",
      "width:1px",
      "height:1px",
      "opacity:0",
      "resize:none",
      "border:0",
      "padding:0",
      "margin:0",
      "outline:none",
      "overflow:hidden",
      "z-index:1",
      "pointer-events:none",
    ].join(";") + ";";
    container.appendChild(textarea);

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
      textarea,
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
      selection: null,
      isDragging: false,
      hoveredHyperlink: null,
      composing: false,
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
  // ── Keyboard: lives on the hidden textarea so we get composition events ──

  entry.textarea.addEventListener("keydown", (e) => {
    // Bare modifier / dead-key presses never reach the PTY. Without this,
    // `key="Control"` would fall into the backend's Char fallback and send
    // Ctrl+C to the shell on every Control keypress.
    if (isModifierOrDead(e)) return;

    // While the browser is composing an IME sequence, let the textarea
    // accumulate; we'll forward the result via `compositionend`.
    if (e.isComposing || entry.composing) return;

    // Terminal-scoped copy/paste wins over the global pass-through list.
    if (e.ctrlKey && e.shiftKey && !e.altKey && e.code === "KeyC" && entry.selection) {
      e.preventDefault();
      void copySelection(entry);
      return;
    }
    if (e.ctrlKey && e.shiftKey && !e.altKey && e.code === "KeyV") {
      e.preventDefault();
      void pasteFromClipboard(entry);
      return;
    }

    if (shouldPassThrough(e)) return;

    // Unmodified printable keys go through the `input` event instead so
    // the textarea's composition state stays coherent with the browser —
    // anything else (modified keys, arrows, Enter, etc.) bypasses the
    // textarea and is encoded by the backend directly.
    if (!isSpecialOrModified(e)) return;

    // Any typing implicitly commits the selection — matches every mainstream
    // terminal's "type to clear highlight" behavior.
    if (entry.selection) {
      entry.selection = null;
      paintAll(entry);
    }

    e.preventDefault();
    sendKeyEvent(entry, e);
  });

  entry.textarea.addEventListener("input", (e) => {
    const ie = e as InputEvent;
    if (ie.isComposing || entry.composing) return;
    const text = entry.textarea.value;
    entry.textarea.value = "";
    if (!text) return;
    sendText(entry, text);
  });

  entry.textarea.addEventListener("compositionstart", () => {
    entry.composing = true;
  });

  entry.textarea.addEventListener("compositionend", (e) => {
    entry.composing = false;
    const composed = e.data ?? entry.textarea.value;
    entry.textarea.value = "";
    if (composed) {
      sendText(entry, composed);
    }
  });

  // ── Mouse: lives on the visible canvas ──

  entry.canvas.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    focusCanvas(entry);
    const cell = mouseToCell(entry, e);
    if (!cell) return;

    // Click on a hyperlink cell (without a drag) opens it. We distinguish
    // click-vs-drag by comparing mousedown and mouseup cell positions in
    // the mouseup handler below.
    entry.isDragging = true;
    entry.selection = { anchor: cell, head: cell };
    paintAll(entry);
  });

  entry.canvas.addEventListener("mousemove", (e) => {
    const cell = mouseToCell(entry, e);
    if (!cell) return;

    if (entry.isDragging && entry.selection) {
      entry.selection = { anchor: entry.selection.anchor, head: cell };
      paintAll(entry);
      return;
    }

    // Hover: detect hyperlink under the cursor and flip the pointer.
    const hyperlink = entry.grid[cell.row]?.[cell.col]?.hyperlink ?? null;
    if (hyperlink !== entry.hoveredHyperlink) {
      entry.hoveredHyperlink = hyperlink;
      entry.canvas.style.cursor = hyperlink ? "pointer" : "default";
    }
  });

  entry.canvas.addEventListener("mouseup", (e) => {
    if (!entry.isDragging) return;
    entry.isDragging = false;

    if (entry.selection) {
      const { anchor, head } = entry.selection;
      const isClick = anchor.col === head.col && anchor.row === head.row;
      if (isClick) {
        // No drag happened — treat as a plain click. Clear the selection
        // (a one-cell "selection" is not useful) and resolve the hyperlink
        // if the click landed on one.
        entry.selection = null;
        const cell = mouseToCell(entry, e);
        const hyperlink = cell ? entry.grid[cell.row]?.[cell.col]?.hyperlink : null;
        if (hyperlink) {
          openShell(hyperlink).catch((err: unknown) => {
            console.error("shell.open hyperlink failed", err);
          });
        }
        paintAll(entry);
      }
    }
  });

  // Releasing outside the canvas must still end the drag so we don't get
  // stuck with isDragging=true if the user rolls off the pane mid-drag.
  entry.canvas.addEventListener("mouseleave", () => {
    entry.isDragging = false;
  });
}

function mouseToCell(entry: Entry, e: MouseEvent): CellCoord | null {
  const rect = entry.canvas.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  if (x < 0 || y < 0) return null;
  const col = Math.floor(x / entry.metrics.cssWidth);
  const row = Math.floor(y / entry.metrics.cssHeight);
  if (col >= entry.cols || row >= entry.rows) return null;
  return { col, row };
}

async function copySelection(entry: Entry): Promise<void> {
  const text = serializeSelection(entry);
  if (!text) return;
  try {
    // Tauri plugin routes through IPC, sidestepping WebKit's
    // user-gesture-only clipboard policy that silently blocked
    // `navigator.clipboard.writeText` for us.
    await writeClipboard(text);
  } catch (err) {
    console.error("clipboard write failed", err);
  }
}

async function pasteFromClipboard(entry: Entry): Promise<void> {
  try {
    const text = await readClipboard();
    if (!text) return;
    await invoke("terminal_paste", { sessionId: entry.sessionId, text });
  } catch (err) {
    console.error("clipboard read / paste failed", err);
  }
}

/// Walk the selection rectangle row-by-row, trimming trailing blanks per row
/// (matches how xterm.js and wezterm both serialize selected regions).
function serializeSelection(entry: Entry): string {
  if (!entry.selection) return "";
  const { startRow, endRow, startCol, endCol } = orderSelection(entry.selection);
  const lines: string[] = [];
  for (let row = startRow; row <= endRow; row += 1) {
    const cells = entry.grid[row];
    if (!cells) continue;
    const from = row === startRow ? startCol : 0;
    const to = row === endRow ? endCol : entry.cols - 1;
    let text = "";
    for (let col = from; col <= to; col += 1) {
      text += cells[col]?.ch ?? " ";
    }
    lines.push(text.replace(/ +$/, ""));
  }
  return lines.join("\n");
}

function orderSelection(sel: Selection): {
  startRow: number;
  endRow: number;
  startCol: number;
  endCol: number;
} {
  const { anchor, head } = sel;
  if (anchor.row < head.row || (anchor.row === head.row && anchor.col <= head.col)) {
    return {
      startRow: anchor.row,
      endRow: head.row,
      startCol: anchor.col,
      endCol: head.col,
    };
  }
  return {
    startRow: head.row,
    endRow: anchor.row,
    startCol: head.col,
    endCol: anchor.col,
  };
}

function isPrintable(key: string): boolean {
  if (key.length !== 1) return false;
  const code = key.charCodeAt(0);
  return code >= 0x20 && code !== 0x7f;
}

/// A keydown is "special or modified" when it can't be safely handed to the
/// textarea's `input` event — either because it has a non-text meaning
/// (arrows, Enter, Backspace, function keys) or because modifiers change
/// what the shell should receive (Ctrl+A, Alt+letter, etc.). Those bypass
/// the textarea and go straight to the wezterm encoder on the backend.
function isSpecialOrModified(e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.altKey || e.metaKey) return true;
  if (e.key.length !== 1) return true;
  return false;
}

function sendKeyEvent(entry: Entry, e: KeyboardEvent): void {
  const payload: TerminalKeyEvent = {
    code: e.code,
    key: e.key,
    text: isPrintable(e.key) ? e.key : undefined,
    ctrl: e.ctrlKey,
    shift: e.shiftKey,
    alt: e.altKey,
    meta: e.metaKey,
  };
  invoke("terminal_input", { sessionId: entry.sessionId, event: payload }).catch(
    (err: unknown) => console.error("terminal_input failed", err),
  );
}

/// Send a composed string of text (from an IME `input` event or
/// `compositionend`). Encoded as a single-char-per-call stream so the
/// backend's encoder sees each grapheme independently — matches how native
/// keyboard input arrives.
function sendText(entry: Entry, text: string): void {
  for (const ch of text) {
    const payload: TerminalKeyEvent = {
      code: "IME",
      key: ch,
      text: ch,
      ctrl: false,
      shift: false,
      alt: false,
      meta: false,
    };
    invoke("terminal_input", { sessionId: entry.sessionId, event: payload }).catch(
      (err: unknown) => console.error("terminal_input failed", err),
    );
  }
  if (entry.selection) {
    entry.selection = null;
    paintAll(entry);
  }
}

/// Keys that carry no user intent for the PTY and must be dropped before
/// invoking `terminal_input`. Includes lone modifier keydowns (whose `key`
/// is the modifier's name and would otherwise be letter-mapped — e.g.
/// bare Ctrl as `Char('C')` which the backend encodes as Ctrl+C = SIGINT),
/// dead keys (accent lead-ins), and browser placeholders.
const DROPPED_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

const DROPPED_KEYS = new Set([
  "Dead",
  "Unidentified",
  "Process",
  "Control",
  "Shift",
  "Alt",
  "Meta",
  "Super",
]);

function isModifierOrDead(e: KeyboardEvent): boolean {
  return DROPPED_CODES.has(e.code) || DROPPED_KEYS.has(e.key);
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
  // Input focus lives on the hidden textarea so IME/composition events
  // reach us; the canvas is purely visual + mouse.
  entry.textarea.focus({ preventScroll: true });
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
  // `paintRow` overwrites the row entirely, so any selection tint on those
  // rows needs to be re-applied before the cursor draws on top.
  paintSelection(entry);
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
  paintSelection(entry);
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

function paintSelection(entry: Entry): void {
  if (!entry.selection) return;
  // Skip zero-size "selections" — one-cell mousedown without a drag.
  const { anchor, head } = entry.selection;
  if (anchor.col === head.col && anchor.row === head.row) return;

  const { startRow, endRow, startCol, endCol } = orderSelection(entry.selection);
  const { ctx, metrics, cols } = entry;

  ctx.fillStyle = WEZ_THEME.selectionBackground;
  for (let row = startRow; row <= endRow; row += 1) {
    const from = row === startRow ? startCol : 0;
    const to = row === endRow ? endCol : cols - 1;
    const dx = from * metrics.cellWidth;
    const dy = row * metrics.cellHeight;
    const width = (to - from + 1) * metrics.cellWidth;
    ctx.fillRect(dx, dy, width, metrics.cellHeight);
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
