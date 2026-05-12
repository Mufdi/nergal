/// Canvas-based terminal renderer backed by the `wezterm-term` VT emulator
/// running in the Rust backend. State lives outside React's lifecycle: a
/// single host `<div>` is swapped via `show/destroy` and each session owns
/// a canvas entry.
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
import { readText as readClipboard } from "@tauri-apps/plugin-clipboard-manager";
import type { CellSnapshot, GridUpdate, TerminalKeyEvent } from "@/lib/types";
import { appStore } from "@/stores/jotaiStore";
import { toastsAtom } from "@/stores/toast";

import { FontAtlas, isWideCell, measureFont, type FontMetrics } from "./fontAtlas";
import { TERM_FONT, TERM_THEME, refreshTermTheme, rgbaToCss } from "./theme";

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
  /// Floating "you're scrolled up" pill, shown only when scrollOffset > 0.
  scrollIndicator: HTMLDivElement;
  ctx: CanvasRenderingContext2D;
  atlas: FontAtlas;
  metrics: FontMetrics;
  cols: number;
  rows: number;
  totalRows: number;
  grid: CellSnapshot[][];
  cursor: { x: number; y: number; visible: boolean };
  title: string | null;
  scrollOffset: number;
  isAltScreen: boolean;
  unlisten: UnlistenFn;
  selection: Selection | null;
  // scrollOffset at selection time, used by the primary-screen delta sync.
  selectionScrollOffset: number;
  // Text of the anchor row at selection time. Lets us re-find the same
  // content in a later grid (alt-screen TUI redraws have no scrollOffset
  // change to key off of) and shift the selection to follow it.
  selectionAnchorText: string | null;
  // False once the anchor text disappears from the viewport in alt screen;
  // toggled back on when the content scrolls back into view. paintSelection
  // skips while false instead of leaving a stale highlight on top of
  // unrelated content.
  selectionVisible: boolean;
  isDragging: boolean;
  hoveredHyperlink: string | null;
  composing: boolean;
  /// Last composition result + timestamp. Some browsers fire a trailing
  /// keydown with the composed char after `compositionend`; we suppress
  /// that single ghost event instead of blanket-suppressing keydowns for
  /// a long window (which is what dropped the user's fast-typing chars).
  lastComposed: { text: string; at: number } | null;
}

// ── Module state ──

const entries = new Map<string, Entry>();
const pending = new Set<string>();
let hostElement: HTMLDivElement | null = null;
let activeId: string | null = null;

// Prime TERM_THEME from CSS tokens before the first session can paint.
// The atlas keys glyphs by `(ch, fg, …)` so a later color flip lazily
// rasterizes new variants — no manual cache invalidation needed.
refreshTermTheme();

if (typeof document !== "undefined") {
  const observer = new MutationObserver(() => {
    refreshTermTheme();
    const dpr = window.devicePixelRatio || 1;
    for (const entry of entries.values()) {
      // The container's inline `background` was set imperatively at create
      // time; repoint it so the area outside the canvas (during fit() races)
      // matches the new surface.
      entry.container.style.background = TERM_THEME.background;
      // If the theme also swapped the terminal font family, the cached
      // metrics + atlas were keyed to the previous family. Rebuild both
      // and refit the canvas so glyphs render at the right cell size.
      const nextMetrics = measureFont(
        TERM_FONT.family,
        TERM_FONT.size,
        TERM_FONT.lineHeight,
        dpr,
      );
      // Different font families almost always produce different cell
      // metrics — using metrics as the change signal avoids tracking the
      // previous family string explicitly.
      const fontChanged =
        nextMetrics.cellWidth !== entry.metrics.cellWidth ||
        nextMetrics.cellHeight !== entry.metrics.cellHeight;
      if (fontChanged) {
        entry.metrics = nextMetrics;
        entry.atlas = new FontAtlas(
          nextMetrics,
          TERM_FONT.family,
          TERM_FONT.size,
          dpr,
        );
        fit(entry);
      }
      paintAll(entry);
    }
  });
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
}

// ── Public API ──

export function setHost(el: HTMLDivElement | null): void {
  hostElement = el;
  // React can re-mount the host (e.g. after a layout swap). Existing
  // session containers were attached to the previous host — re-parent them
  // to the new one so the terminal remains visible.
  if (el) {
    for (const entry of entries.values()) {
      if (entry.container.parentElement !== el) {
        el.appendChild(entry.container);
      }
    }
  }
}

export async function show(
  sessionId: string,
  cwd: string,
  mode: "new" | "continue" = "new",
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
      TERM_THEME.background +
      ";";
    hostElement.appendChild(container);

    await waitForLayout(container);

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

    const scrollIndicator = document.createElement("div");
    scrollIndicator.textContent = "Scrolled — type or press End to return";
    scrollIndicator.style.cssText = [
      "position:absolute",
      "top:8px",
      "right:12px",
      "padding:4px 10px",
      "border-radius:9999px",
      "font-family:" + TERM_FONT.family,
      "font-size:11px",
      "color:#fff",
      "background:rgba(20,20,20,0.85)",
      "border:1px solid rgba(255,255,255,0.15)",
      "pointer-events:none",
      "z-index:2",
      "display:none",
      "user-select:none",
    ].join(";") + ";";
    container.appendChild(scrollIndicator);

    const dpr = window.devicePixelRatio || 1;
    const metrics = measureFont(TERM_FONT.family, TERM_FONT.size, TERM_FONT.lineHeight, dpr);
    const atlas = new FontAtlas(metrics, TERM_FONT.family, TERM_FONT.size, dpr);

    const { cols, rows } = computeCols(container, metrics);
    sizeCanvas(canvas, cols, rows, metrics);

    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("terminal canvas 2d context unavailable");

    const entry: Entry = {
      sessionId,
      container,
      canvas,
      textarea,
      scrollIndicator,
      ctx,
      atlas,
      metrics,
      cols,
      rows,
      totalRows: rows,
      grid: emptyGrid(cols, rows),
      cursor: { x: 0, y: 0, visible: true },
      title: null,
      scrollOffset: 0,
      isAltScreen: false,
      unlisten: () => {},
      selection: null,
      selectionScrollOffset: 0,
      selectionAnchorText: null,
      selectionVisible: true,
      isDragging: false,
      hoveredHyperlink: null,
      composing: false,
      lastComposed: null,
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

/// Forward a synthesized special key (e.g. Tab, Shift+Tab) to the active
/// PTY without requiring the textarea to own DOM focus. Used by the global
/// shortcut dispatcher when a terminal-reserved key fires while focus lives
/// in a different zone.
export function sendSpecialKeyToActive(code: string, key: string, modifiers: { ctrl?: boolean; shift?: boolean; alt?: boolean } = {}): void {
  if (!activeId) return;
  const entry = entries.get(activeId);
  if (!entry) return;
  const payload: TerminalKeyEvent = {
    code,
    key,
    text: undefined,
    ctrl: modifiers.ctrl ?? false,
    shift: modifiers.shift ?? false,
    alt: modifiers.alt ?? false,
    meta: false,
  };
  invoke("terminal_input", { sessionId: entry.sessionId, event: payload }).catch(
    (err: unknown) => console.error("terminal_input failed", err),
  );
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

    // Ghost keydown trailing a composition: some browsers fire a final
    // keydown with the composed character right after compositionend.
    // Suppress exactly one, only when it matches, within a tight window.
    if (
      entry.lastComposed
      && e.key === entry.lastComposed.text
      && Date.now() - entry.lastComposed.at < 60
    ) {
      entry.lastComposed = null;
      e.preventDefault();
      return;
    }

    // Ctrl+Shift+C stays with cluihud's global `commit-session` binding —
    // we cannot block it here anyway because `useKeyboardShortcuts` listens
    // in capture phase, ahead of our bubble-phase listener. Instead, copy
    // is triggered automatically on selection release (see mouseup).
    if (e.ctrlKey && !e.altKey && e.code === "KeyV") {
      e.preventDefault();
      e.stopPropagation();
      void pasteFromClipboard(entry);
      return;
    }

    // Shift+End and Escape (while scrolled) snap the viewport back to the
    // live bottom without sending any byte to the PTY. Plain End is left
    // alone so shells still treat it as "move cursor to end of line".
    if (
      (e.code === "End" && e.shiftKey)
      || (e.code === "Escape" && entry.scrollOffset > 0)
    ) {
      e.preventDefault();
      invoke("terminal_scroll_to_bottom", { sessionId: entry.sessionId }).catch(
        (err: unknown) => console.error("terminal_scroll_to_bottom failed", err),
      );
      return;
    }

    if (shouldPassThrough(e)) return;

    // Any typing implicitly commits the selection — matches every mainstream
    // terminal's "type to clear highlight" behavior.
    if (entry.selection) {
      entry.selection = null;
      paintAll(entry);
    }

    e.preventDefault();
    sendKeyEvent(entry, e);
    // Keep the textarea empty so the browser never paints a stray char
    // under our canvas nor accumulates stale state.
    entry.textarea.value = "";
  });

  entry.textarea.addEventListener("compositionstart", () => {
    entry.composing = true;
  });

  entry.textarea.addEventListener("compositionend", (e) => {
    entry.composing = false;
    const composed = e.data ?? entry.textarea.value;
    entry.textarea.value = "";
    if (composed) {
      entry.lastComposed = { text: composed, at: Date.now() };
      sendText(entry, composed);
    }
  });

  // ── Mouse: lives on the visible canvas ──

  entry.canvas.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    // Block the browser's default focus shift so our explicit textarea.focus()
    // call below sticks — otherwise Chromium/WebKit would blur the textarea
    // immediately after the handler returns, since the canvas itself isn't a
    // focusable element. Native selection isn't used here (we track selection
    // via grid cells manually), so suppressing the default is harmless.
    e.preventDefault();
    focusCanvas(entry);
    const cell = mouseToCell(entry, e);
    if (!cell) return;

    // Click on a hyperlink cell (without a drag) opens it. We distinguish
    // click-vs-drag by comparing mousedown and mouseup cell positions in
    // the mouseup handler below.
    entry.isDragging = true;
    entry.selection = { anchor: cell, head: cell };
    entry.selectionScrollOffset = entry.scrollOffset;
    entry.selectionAnchorText = rowText(entry, cell.row);
    entry.selectionVisible = true;
    paintAll(entry);
  });

  entry.canvas.addEventListener("mousemove", (e) => {
    const cell = mouseToCell(entry, e);
    if (!cell) return;

    if (entry.isDragging && entry.selection) {
      const delta = entry.scrollOffset - entry.selectionScrollOffset;
      const head = { row: cell.row - delta, col: cell.col };
      entry.selection = { anchor: entry.selection.anchor, head };
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

    if (!entry.selection) return;
    const { anchor, head } = entry.selection;
    const isClick = anchor.col === head.col && anchor.row === head.row;

    if (isClick) {
      // No drag happened — treat as a plain click. Clear the one-cell
      // "selection" (not useful) and resolve the hyperlink if the click
      // landed on one.
      entry.selection = null;
      const cell = mouseToCell(entry, e);
      const hyperlink = cell ? entry.grid[cell.row]?.[cell.col]?.hyperlink : null;
      if (hyperlink) {
        openShell(hyperlink).catch((err: unknown) => {
          console.error("shell.open hyperlink failed", err);
        });
      }
      paintAll(entry);
      return;
    }

    // Real drag ended — Ghostty-style auto-copy: the selected text goes
    // straight to the clipboard and a toast confirms. Sidesteps the
    // Ctrl+Shift+C keyboard shortcut entirely (which cluihud binds to
    // commit-session globally and which we can't intercept because the
    // shortcut dispatcher listens in capture phase).
    const text = serializeSelection(entry);
    if (!text) return;

    // The toast mounts a focusable <button> at the viewport edge; in
    // WebKitGTK that can steal focus away from our textarea when a
    // layout-triggered paint moves the focused element momentarily
    // outside the hit region. Fire the toast first and schedule a
    // follow-up refocus on the next frame. Also clipboard-write is async
    // so typing can continue in parallel.
    appStore.set(toastsAtom, {
      message: "Copied to clipboard",
      type: "success",
    });
    // Own command (spawn_blocking) instead of plugin-clipboard-manager —
    // avoids stalling the tokio runtime on Wayland systems where arboard's
    // blocking wl-clipboard I/O would otherwise delay subsequent async
    // commands like terminal_input.
    invoke("terminal_clipboard_write", { text }).catch((err: unknown) => {
      console.error("auto-copy failed", err);
    });
    // Two-phase refocus: one immediately, one after a RAF (post sileo's
    // enter animation starts). Safe to call even if focus is already on
    // the textarea — it's a no-op in that case.
    entry.textarea.focus({ preventScroll: true });
    requestAnimationFrame(() => {
      entry.textarea.focus({ preventScroll: true });
    });
  });

  // Releasing outside the canvas must still end the drag so we don't get
  // stuck with isDragging=true if the user rolls off the pane mid-drag.
  entry.canvas.addEventListener("mouseleave", () => {
    entry.isDragging = false;
  });

  entry.canvas.addEventListener("dblclick", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const cell = mouseToCell(entry, e);
    if (!cell) return;
    const word = wordRangeAt(entry, cell);
    if (!word) return;
    entry.selection = {
      anchor: { row: cell.row, col: word.startCol },
      head: { row: cell.row, col: word.endCol },
    };
    entry.selectionScrollOffset = entry.scrollOffset;
    entry.selectionAnchorText = rowText(entry, cell.row);
    entry.selectionVisible = true;
    paintAll(entry);
    const text = serializeSelection(entry);
    if (!text) return;
    appStore.set(toastsAtom, { message: "Copied to clipboard", type: "success" });
    invoke("terminal_clipboard_write", { text }).catch((err: unknown) => {
      console.error("auto-copy failed", err);
    });
    entry.textarea.focus({ preventScroll: true });
  });

  // ── Wheel: scrollback (primary screen) or PTY forward (alt screen) ──
  // The viewport is canvas-only — the browser has no native scroll. We
  // translate wheel pixels into terminal lines and let the backend route:
  // primary screen → local scrollback navigation, alt screen → mouse
  // report or arrow-key fallback to the running TUI app (claude, vim, …).
  entry.canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      let lines: number;
      switch (e.deltaMode) {
        case WheelEvent.DOM_DELTA_LINE:
          lines = Math.round(e.deltaY);
          break;
        case WheelEvent.DOM_DELTA_PAGE:
          lines = Math.round(e.deltaY * entry.rows);
          break;
        default:
          lines = Math.round(e.deltaY / entry.metrics.cssHeight);
          break;
      }
      if (lines === 0) return;
      const cell = mouseToCell(entry, e) ?? { col: 0, row: 0 };
      invoke("terminal_scroll", {
        sessionId: entry.sessionId,
        delta: -lines,
        col: cell.col,
        row: cell.row,
      }).catch((err: unknown) => console.error("terminal_scroll failed", err));
    },
    { passive: false },
  );
}

function rowText(entry: Entry, row: number): string | null {
  const cells = entry.grid[row];
  if (!cells) return null;
  let text = "";
  for (const c of cells) text += c.ch ?? " ";
  return text.replace(/ +$/, "");
}

function syncSelectionToContent(entry: Entry): void {
  if (!entry.isAltScreen || !entry.selection || !entry.selectionAnchorText) return;
  const target = entry.selectionAnchorText;
  const oldAnchorRow = entry.selection.anchor.row;
  let bestRow = -1;
  let bestDistance = Infinity;
  for (let r = 0; r < entry.rows; r += 1) {
    if (rowText(entry, r) !== target) continue;
    const d = Math.abs(r - oldAnchorRow);
    if (d < bestDistance) {
      bestDistance = d;
      bestRow = r;
    }
  }
  if (bestRow === -1) {
    entry.selectionVisible = false;
    return;
  }
  entry.selectionVisible = true;
  const delta = bestRow - oldAnchorRow;
  if (delta === 0) return;
  entry.selection = {
    anchor: { row: entry.selection.anchor.row + delta, col: entry.selection.anchor.col },
    head: { row: entry.selection.head.row + delta, col: entry.selection.head.col },
  };
}

function wordRangeAt(entry: Entry, cell: CellCoord): { startCol: number; endCol: number } | null {
  const row = entry.grid[cell.row];
  if (!row) return null;
  const ch = row[cell.col]?.ch;
  if (!ch || ch === " ") return null;
  let startCol = cell.col;
  while (startCol > 0) {
    const prev = row[startCol - 1]?.ch;
    if (!prev || prev === " ") break;
    startCol -= 1;
  }
  let endCol = cell.col;
  while (endCol < entry.cols - 1) {
    const next = row[endCol + 1]?.ch;
    if (!next || next === " ") break;
    endCol += 1;
  }
  return { startCol, endCol };
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

async function pasteFromClipboard(entry: Entry): Promise<void> {
  try {
    const text = await readClipboard().catch(() => "");
    if (text && text.length > 0) {
      await invoke("terminal_paste", { sessionId: entry.sessionId, text });
      return;
    }
    // No text in clipboard: forward Ctrl+V (\x16) so the underlying agent CLI
    // can handle non-text payloads (image paste in Claude Code, etc.).
    await invoke("write_to_session_pty", { sessionId: entry.sessionId, data: "\x16" });
  } catch (err) {
    console.error("clipboard read / paste failed", err);
  }
}

/// Walk the selection rectangle row-by-row, trimming trailing blanks per row
/// (matches how xterm.js and wezterm both serialize selected regions).
function serializeSelection(entry: Entry): string {
  if (!entry.selection) return "";
  const { startRow, endRow, startCol, endCol } = orderSelection(entry.selection);
  const delta = entry.scrollOffset - entry.selectionScrollOffset;
  const lines: string[] = [];
  for (let row = startRow; row <= endRow; row += 1) {
    const displayRow = row + delta;
    const cells = entry.grid[displayRow];
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
  const rawCols = Math.floor(rect.width / metrics.cssWidth);
  const rawRows = Math.floor(rect.height / metrics.cssHeight);
  // Guard against a not-yet-laid-out container. Launching a shell at 1x1
  // is unrecoverable: many TUIs (including `claude`) cache dimensions at
  // startup and never reflow properly even after SIGWINCH. Default to a
  // conventional 80x24 when the measurement looks bogus.
  const cols = rawCols >= 10 ? rawCols : 80;
  const rows = rawRows >= 3 ? rawRows : 24;
  return { cols, rows };
}

/// Poll the container until it has real dimensions, capped at ~1s. A few
/// RAFs are usually enough; in rare cases (pane animated in, React Suspense
/// boundary just resolved) the extra budget prevents us from seeding the
/// PTY with placeholder dimensions.
async function waitForLayout(container: HTMLElement): Promise<void> {
  for (let i = 0; i < 60; i += 1) {
    const rect = container.getBoundingClientRect();
    if (rect.width >= 100 && rect.height >= 50) return;
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }
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
  if (entry.scrollOffset !== update.scrollOffset) {
    entry.scrollOffset = update.scrollOffset;
    entry.scrollIndicator.style.display = update.scrollOffset > 0 ? "block" : "none";
  }
  entry.isAltScreen = update.isAltScreen;
  syncSelectionToContent(entry);

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
  entry.ctx.fillStyle = TERM_THEME.background;
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
  ctx.fillStyle = TERM_THEME.background;
  ctx.fillRect(0, dyCell, cols * metrics.cellWidth, metrics.cellHeight);

  for (let x = 0; x < cols; x += 1) {
    const cell = row[x];
    const dxCell = x * metrics.cellWidth;
    const span = isWideCell(cell.ch) ? 2 : 1;
    const cellPx = metrics.cellWidth * span;

    let fg = rgbaToCss(cell.fg, TERM_THEME.foreground);
    let bg = rgbaToCss(cell.bg, TERM_THEME.background);
    if (cell.reverse) {
      const swap = fg;
      fg = bg;
      bg = swap;
    }

    if (bg !== TERM_THEME.background) {
      ctx.fillStyle = bg;
      ctx.fillRect(dxCell, dyCell, cellPx, metrics.cellHeight);
    }

    if (cell.ch && cell.ch !== " ") {
      atlas.drawGlyph(ctx, dxCell, dyCell, cell.ch, fg, cell.bold, cell.italic);
    }

    if (cell.underline) {
      ctx.fillStyle = fg;
      ctx.fillRect(
        dxCell,
        dyCell + metrics.baseline + 1,
        cellPx,
        Math.max(1, Math.floor(metrics.cellHeight / 16)),
      );
    }

    // Wezterm emits a blank placeholder in the column right after a
    // wide cell. Skip it so we do not paint over the right half of the
    // emoji we just drew.
    if (span === 2) x += 1;
  }
}

function paintSelection(entry: Entry): void {
  if (!entry.selection) return;
  if (!entry.selectionVisible) return;
  const { anchor, head } = entry.selection;
  if (anchor.col === head.col && anchor.row === head.row) return;

  const { startRow, endRow, startCol, endCol } = orderSelection(entry.selection);
  const { ctx, metrics, cols, rows } = entry;
  const delta = entry.scrollOffset - entry.selectionScrollOffset;

  ctx.fillStyle = TERM_THEME.selectionBackground;
  for (let row = startRow; row <= endRow; row += 1) {
    const displayRow = row + delta;
    if (displayRow < 0 || displayRow >= rows) continue;
    const from = row === startRow ? startCol : 0;
    const to = row === endRow ? endCol : cols - 1;
    const dx = from * metrics.cellWidth;
    const dy = displayRow * metrics.cellHeight;
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

  ctx.fillStyle = TERM_THEME.cursor;
  ctx.fillRect(dx, dy, metrics.cellWidth, metrics.cellHeight);

  const cell = entry.grid[cursor.y]?.[cursor.x];
  if (cell?.ch && cell.ch !== " ") {
    entry.atlas.drawGlyph(
      ctx,
      dx,
      dy,
      cell.ch,
      TERM_THEME.cursorAccent,
      cell.bold,
      cell.italic,
    );
  }
}
