/// Shared keymap primitives. Single source of truth for translating between
/// the registry's keys string ("ctrl+shift+b"), the physical `event.code`
/// matched at dispatch time (WebKitGTK requires code, not key), and the
/// capture flow in Settings → Keymap. The dispatcher, command palette, and
/// keymap editor all import from here so parsing never drifts.

export const KEY_TO_CODE: Record<string, string> = {
  a: "KeyA", b: "KeyB", c: "KeyC", d: "KeyD", e: "KeyE",
  f: "KeyF", g: "KeyG", h: "KeyH", i: "KeyI", j: "KeyJ",
  k: "KeyK", l: "KeyL", m: "KeyM", n: "KeyN", o: "KeyO",
  p: "KeyP", q: "KeyQ", r: "KeyR", s: "KeyS", t: "KeyT",
  u: "KeyU", v: "KeyV", w: "KeyW", x: "KeyX", y: "KeyY",
  z: "KeyZ",
  "1": "Digit1", "2": "Digit2", "3": "Digit3",
  "4": "Digit4", "5": "Digit5", "6": "Digit6",
  "7": "Digit7", "8": "Digit8", "9": "Digit9",
  "0": "Digit0",
  f1: "F1", f2: "F2", f3: "F3", f4: "F4", f5: "F5", f6: "F6",
  f7: "F7", f8: "F8", f9: "F9", f10: "F10", f11: "F11", f12: "F12",
  tab: "Tab",
  enter: "Enter",
  backspace: "Backspace",
  arrowleft: "ArrowLeft", arrowright: "ArrowRight",
  arrowup: "ArrowUp", arrowdown: "ArrowDown",
  pagedown: "PageDown", pageup: "PageUp",
  home: "Home", end: "End",
  // `ñ` resolves to the Semicolon physical key (Spanish layout) — the same
  // mapping focus-terminal (Ctrl+Ñ) already relies on.
  ñ: "Semicolon",
  ",": "Comma", ".": "Period", "/": "Slash",
};

/// Reverse map for capture: `event.code` → the registry token. Only one token
/// owns each code (ñ owns Semicolon), so this inversion is unambiguous.
const CODE_TO_KEY: Record<string, string> = Object.fromEntries(
  Object.entries(KEY_TO_CODE).map(([token, code]) => [code, token]),
);

export interface ParsedShortcut {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  code: string;
}

export function parseKeys(keys: string): ParsedShortcut | null {
  const parts = keys.toLowerCase().split("+");
  const key = parts[parts.length - 1];
  const code = KEY_TO_CODE[key];
  if (!code) return null;
  return {
    ctrl: parts.includes("ctrl"),
    shift: parts.includes("shift"),
    alt: parts.includes("alt"),
    code,
  };
}

/// Canonical signature for collision comparison. Two keys strings collide iff
/// their signatures are equal. Returns null for un-parseable combos (e.g. the
/// quake default "ctrl+}", whose `}` glyph is layout-dependent and matched by
/// a dedicated dual-key/code handler rather than the generic parser).
export function comboSignature(keys: string): string | null {
  const p = parseKeys(keys);
  if (!p) return null;
  return `${p.ctrl ? "c" : ""}${p.shift ? "s" : ""}${p.alt ? "a" : ""}:${p.code}`;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

/// Translate a live KeyboardEvent into a registry keys string during capture.
/// Returns null when the press is a bare modifier or a key we can't represent
/// (so the capture UI can ignore it and keep listening).
export function eventToKeys(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const token = CODE_TO_KEY[e.code];
  if (!token) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("ctrl");
  if (e.shiftKey) parts.push("shift");
  if (e.altKey) parts.push("alt");
  parts.push(token);
  return parts.join("+");
}

/// Pretty key tokens for display (kbd badges). Shared by the command palette
/// and the keymap editor so they render identically.
export function formatKeyParts(keys: string): string[] {
  return keys.split("+").map((p) => {
    switch (p) {
      case "ctrl": return "Ctrl";
      case "shift": return "Shift";
      case "alt": return "Alt";
      case "tab": return "Tab";
      case "enter": return "Enter";
      case "backspace": return "Backspace";
      case "arrowleft": return "←";
      case "arrowright": return "→";
      case "arrowup": return "↑";
      case "arrowdown": return "↓";
      case "pagedown": return "PgDn";
      case "pageup": return "PgUp";
      case "ñ": return "Ñ";
      default: return p.toUpperCase();
    }
  });
}

/// Shortcuts whose binding is structural and never remappable: the command
/// palette (the escape hatch to every other command), terminal focus, and the
/// 1-9 session switches (muscle memory + their Ctrl+Shift mirror). Overrides
/// for these ids are ignored even if hand-edited into config.json.
export const LOCKED_SHORTCUT_IDS = new Set<string>([
  "command-palette",
  "focus-terminal",
  "session-1", "session-2", "session-3", "session-4", "session-5",
  "session-6", "session-7", "session-8", "session-9",
]);

/// Combos reserved by the OS / desktop environment — binding onto them would be
/// shadowed. Compared by signature. Ctrl+Shift+U is IBus unicode input on Linux.
const RESERVED_SIGNATURES = new Set<string>(
  ["ctrl+shift+u"].map((k) => comboSignature(k)).filter((s): s is string => s !== null),
);

export interface ComboValidation {
  ok: boolean;
  /// Remediation-style message when `ok` is false.
  reason?: string;
}

/// Validate a freshly captured combo for a target shortcut against the current
/// effective keymap. Enforces: a Ctrl/Alt modifier (bare or Shift-only combos
/// would swallow terminal typing), OS-reserved combos, and collisions with any
/// other shortcut's effective binding.
export function validateCombo(
  keys: string,
  targetId: string,
  effective: { id: string; keys: string }[],
): ComboValidation {
  const parsed = parseKeys(keys);
  if (!parsed) {
    return { ok: false, reason: "Unsupported key. Pick a letter, digit, function or arrow key." };
  }
  if (!parsed.ctrl && !parsed.alt) {
    return { ok: false, reason: "Use Ctrl or Alt — bare or Shift-only combos would interfere with typing in the terminal." };
  }
  const sig = comboSignature(keys);
  if (sig && RESERVED_SIGNATURES.has(sig)) {
    return { ok: false, reason: "Reserved by the system (IBus unicode input). Choose another combo." };
  }
  const clash = effective.find((s) => s.id !== targetId && comboSignature(s.keys) === sig);
  if (clash) {
    return { ok: false, reason: `Already bound to "${clash.id}". Pick a free combo or rebind that one first.` };
  }
  return { ok: true };
}
