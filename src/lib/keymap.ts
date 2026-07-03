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
  space: "Space",
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

const CHORD_PREFIX = "leader ";

/// True for chord strings ("leader n", "leader shift+w"). The `leader` token
/// resolves to the leader's effective binding at dispatch time; here it is
/// just the namespace marker that routes to parseChord instead of parseKeys.
export function isChord(keys: string): boolean {
  return keys.toLowerCase().startsWith(CHORD_PREFIX);
}

/// The combo substring after "leader ", or null for non-chords.
export function chordContinuation(keys: string): string | null {
  return isChord(keys) ? keys.slice(CHORD_PREFIX.length) : null;
}

/// Parses a chord's continuation as a plain combo. Null for non-chords and
/// for continuations parseKeys can't resolve.
export function parseChord(keys: string): ParsedShortcut | null {
  const continuation = chordContinuation(keys);
  return continuation === null ? null : parseKeys(continuation);
}

export function parseKeys(keys: string): ParsedShortcut | null {
  // Chords route through parseChord; "" is the palette-only sentinel. Both
  // must stay unparseable as plain combos, not fall through the lookup below.
  if (keys === "" || isChord(keys)) return null;
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
/// their signatures are equal. Chords get a namespaced `"L:" + <signature>`
/// form so leader continuations never collide with global combos sharing the
/// same underlying key. Returns null for un-parseable combos (e.g. the quake
/// default "ctrl+}", whose `}` glyph is layout-dependent and matched by a
/// dedicated dual-key/code handler rather than the generic parser) and for
/// "" (palette-only).
export function comboSignature(keys: string): string | null {
  if (isChord(keys)) {
    const continuation = chordContinuation(keys);
    const contSig = continuation === null ? null : comboSignature(continuation);
    return contSig === null ? null : `L:${contSig}`;
  }
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

/// Label for the physical Semicolon key. Spanish/LA layouts type Ñ there
/// (the mapping focus-terminal relies on); most other layouts type `;`.
/// Refined at startup by initKeyboardLayoutLabels() where the Keyboard API
/// exists (Chromium-based webviews); WebKitGTK lacks it, so Linux keeps the
/// Ñ default.
let semicolonKeyLabel = "Ñ";

export async function initKeyboardLayoutLabels(): Promise<void> {
  type LayoutMapNavigator = Navigator & {
    keyboard?: { getLayoutMap?: () => Promise<Map<string, string>> };
  };
  const getLayoutMap = (navigator as LayoutMapNavigator).keyboard?.getLayoutMap;
  if (!getLayoutMap) return;
  try {
    const layout = await getLayoutMap.call((navigator as LayoutMapNavigator).keyboard);
    const glyph = layout.get("Semicolon");
    if (glyph) semicolonKeyLabel = glyph.toUpperCase();
  } catch {
    // Layout probe is best-effort cosmetics — the binding matches by code
    // either way, so a failed probe just keeps the Ñ default.
  }
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
      case "space": return "Space";
      case "arrowleft": return "←";
      case "arrowright": return "→";
      case "arrowup": return "↑";
      case "arrowdown": return "↓";
      case "pagedown": return "PgDn";
      case "pageup": return "PgUp";
      case "ñ": return semicolonKeyLabel;
      default: return p.toUpperCase();
    }
  });
}

/// Shortcuts whose binding is structural and never remappable in the keymap
/// editor UI (no Rebind button): the command palette (the escape hatch to
/// every other command) and the 1-9 session switches. Overrides for these ids
/// are ignored at resolution time even if hand-edited into config.json.
/// `leader` and `focus-terminal` were unlocked post-walk (2026-07-03, user
/// decision): they rebind like any other row, protected by the declared
/// RESERVED_COMBOS bans below instead of a lock.
export const LOCKED_SHORTCUT_IDS = new Set<string>([
  "command-palette",
  "session-1", "session-2", "session-3", "session-4", "session-5",
  "session-6", "session-7", "session-8", "session-9",
]);

/// Combos reserved by the OS / desktop environment — binding onto them would
/// be shadowed before the app ever sees the keydown. Compared by signature.
/// Declared bans (docs/shortcuts.md "never-bind" classes, capturable subset):
const RESERVED_COMBOS: [combo: string, reason: string][] = [
  ["ctrl+shift+u", "IBus unicode input on Linux"],
  ["ctrl+alt+t", "GNOME open-terminal"],
  ["ctrl+alt+l", "GNOME lock screen"],
  ["ctrl+alt+arrowleft", "GNOME workspace switch"],
  ["ctrl+alt+arrowright", "GNOME workspace switch"],
  ["ctrl+alt+arrowup", "GNOME workspace switch"],
  ["ctrl+alt+arrowdown", "GNOME workspace switch"],
];
const RESERVED_SIGNATURES = new Map<string, string>(
  RESERVED_COMBOS.flatMap(([combo, reason]) => {
    const sig = comboSignature(combo);
    return sig ? [[sig, reason] as [string, string]] : [];
  }),
);

export interface ComboValidation {
  ok: boolean;
  /// Remediation-style message when `ok` is false.
  reason?: string;
}

/// Collision scan shared by both validateCombo branches. The `L:` namespace
/// prefix in comboSignature already partitions leader continuations from
/// global combos, so a plain equality scan is correct for both.
function findClash(
  sig: string,
  targetId: string,
  effective: { id: string; keys: string }[],
): { id: string; keys: string } | undefined {
  return effective.find((s) => s.id !== targetId && comboSignature(s.keys) === sig);
}

/// Validate a freshly captured combo for a target shortcut against the current
/// effective keymap.
///
/// Plain combos: require a Ctrl/Alt modifier (bare or Shift-only combos would
/// swallow terminal typing), reject OS-reserved combos, and check collisions
/// against any other shortcut's effective binding.
///
/// Leader continuations (`"leader <combo>"`): the prefix already isolates
/// them from terminal typing, so no Ctrl/Alt requirement and no OS-reserved
/// check apply — but Ctrl/Alt in the continuation itself is rejected (sloppy
/// chording is a dispatch-time tolerance, not a bindable combo). Collisions
/// are checked the same way; the namespaced signature keeps them scoped to
/// other leader continuations.
export function validateCombo(
  keys: string,
  targetId: string,
  effective: { id: string; keys: string }[],
): ComboValidation {
  if (isChord(keys)) {
    const continuation = parseChord(keys);
    if (!continuation) {
      return { ok: false, reason: "Unsupported key. Pick a letter, digit, function or arrow key." };
    }
    if (continuation.ctrl || continuation.alt) {
      return { ok: false, reason: "Leader continuations are plain keys or Shift+key — remove Ctrl/Alt." };
    }
    // Bare `.` is the dispatcher's raw-mode sentinel — it resolves before the
    // continuation scan ever runs, so a `leader .` binding could never fire.
    if (continuation.code === "Period" && !continuation.shift) {
      return { ok: false, reason: "The . key is reserved for send-to-terminal (raw mode). Pick another continuation." };
    }
    const sig = comboSignature(keys);
    const clash = sig ? findClash(sig, targetId, effective) : undefined;
    if (clash) {
      return { ok: false, reason: `Already bound to "${clash.id}". Pick a free combo or rebind that one first.` };
    }
    return { ok: true };
  }

  const parsed = parseKeys(keys);
  if (!parsed) {
    return { ok: false, reason: "Unsupported key. Pick a letter, digit, function or arrow key." };
  }
  if (!parsed.ctrl && !parsed.alt) {
    return { ok: false, reason: "Use Ctrl or Alt — bare or Shift-only combos would interfere with typing in the terminal." };
  }
  const sig = comboSignature(keys);
  const reserved = sig ? RESERVED_SIGNATURES.get(sig) : undefined;
  if (reserved) {
    return { ok: false, reason: `Reserved by the system (${reserved}). Choose another combo.` };
  }
  const clash = sig ? findClash(sig, targetId, effective) : undefined;
  if (clash) {
    return { ok: false, reason: `Already bound to "${clash.id}". Pick a free combo or rebind that one first.` };
  }
  return { ok: true };
}
