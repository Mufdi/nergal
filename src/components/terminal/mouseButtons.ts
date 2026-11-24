export type AuxButton = "middle" | "right";

export interface AuxHeld {
  button: number;
  aux: AuxButton;
  col: number;
  row: number;
}

export interface AuxCell {
  col: number;
  row: number;
}

export type AuxEvent =
  | { type: "press"; press: AuxHeld }
  | { type: "release"; button: number; cell: AuxCell | null }
  | { type: "blur" };

export interface AuxSend {
  kind: "press" | "release";
  aux: AuxButton;
  col: number;
  row: number;
}

// Viewport rows only equal live-screen rows when not scrolled into history.
// Shift bypasses reporting so the user keeps the native context menu.
export function reportsMouse(mouseTracking: boolean, scrollOffset: number, shiftKey: boolean): boolean {
  return mouseTracking && scrollOffset === 0 && !shiftKey;
}

// A release while scrolled back has no live-screen cell; the press cell is used instead.
export function releasePointerCell(cell: AuxCell | null, scrollOffset: number): AuxCell | null {
  return scrollOffset === 0 ? cell : null;
}

export function auxButtonToForward(button: number, reportsMouseEvents: boolean): AuxButton | null {
  if (!reportsMouseEvents) return null;
  if (button === 1) return "middle";
  if (button === 2) return "right";
  return null;
}

// One held aux button per terminal: a second press while one is held would
// leave wezterm tracking two buttons whose releases can arrive in any order.
export function auxReduce(held: AuxHeld | null, event: AuxEvent): { held: AuxHeld | null; send: AuxSend | null } {
  switch (event.type) {
    case "press":
      if (held) return { held, send: null };
      return {
        held: event.press,
        send: { kind: "press", aux: event.press.aux, col: event.press.col, row: event.press.row },
      };
    case "release": {
      if (!held || held.button !== event.button) return { held, send: null };
      const at = event.cell ?? held;
      return { held: null, send: { kind: "release", aux: held.aux, col: at.col, row: at.row } };
    }
    case "blur":
      return { held: null, send: null };
  }
}

export type KeyboardMenuKey = "shift-f10" | "menu";

interface KeyLike {
  code: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

export function keyboardMenuKey(e: KeyLike): KeyboardMenuKey | null {
  if (e.ctrlKey || e.altKey || e.metaKey) return null;
  if (e.code === "F10" && e.shiftKey) return "shift-f10";
  if (e.code === "ContextMenu" && !e.shiftKey) return "menu";
  return null;
}

// Shift+F10 is the keyboard escape hatch to the native menu, so it is never
// suppressed; the Menu key carries no Shift, so it follows the right-click rule.
export function suppressContextMenu(args: {
  tracking: boolean;
  offset: number;
  shift: boolean;
  keyboard: KeyboardMenuKey | null;
}): boolean {
  if (args.keyboard === "shift-f10") return false;
  return reportsMouse(args.tracking, args.offset, args.keyboard === "menu" ? false : args.shift);
}
