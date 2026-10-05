# Keyboard shortcuts — layer conventions, never-bind list, deliberate shadows

Nergal's global dispatcher intercepts keydown at window-capture *before* the
agent PTY, so every shortcut it claims permanently shadows that combination
for the agent CLI underneath. This file records the *rules* that keep that
trade-off sane: where a new binding belongs, what must never be bound, and
which shadows are accepted on purpose. The binding table itself — what each
key currently does — lives in `src/stores/shortcuts.ts` (source of truth) and
`docs/patterns.md` §1 (quick reference). Read both before adding or changing a
shortcut.

## The 4-layer model

| Layer | Keys | Allocation rule |
|-------|------|------------------|
| **Core** | bare `Ctrl+{key}` | Reserved for OS/browser/editor-mirroring globals only (session switch, palette, save, close, fullscreen, the leader key itself). Effectively closed — don't add here. |
| **Surfaces** | `Ctrl+Shift+{letter}` | Panels/nouns the user opens dozens of times a session. Nearly saturated (`u` is reserved by IBus, see below); a new panel almost never gets a free slot. |
| **Leader** | `Ctrl+Space` then `{key}` | **Default home for new features.** Flat namespace in v1 — a family gets a direct letter (`leader p`), not a sub-prefix (`leader g p`); depth-2 groups are a documented future evolution, not implemented. |
| **App-scope** | a marked subset of Core/Surfaces | Not a separate keyspace — a `scope: "app"` tag on an existing Core/Surfaces entry that yields to the agent CLI under `keyboard_ownership: "agent"` while terminal/quake holds focus. |

**Promotion rule**: a shortcut is born in the leader. It only moves up to
`Ctrl+Shift` once it has *daily-use evidence* — used constantly enough that
the extra keystroke measurably costs flow, not just "feels important." This
is deliberate friction: the Surfaces tier is small and finite, so promotion
must be earned, not assumed at design time.

## Never-bind list

Nergal must never claim these, globally or in the leader's continuation set,
because they belong to something underneath that has priority. Grouped by
**stable class** (these rot slowly) rather than per-key inventory (that rots
every CLI release — the full per-key audit lives in the vault, see
Provenance below).

- **Terminal signals** — `Ctrl+C` (SIGINT), `Ctrl+D` (EOF), `Ctrl+Z`
  (SIGTSTP), `Ctrl+\` (SIGQUIT). Intercepting any of these interrupts or kills
  the agent process from under the user; this is exactly the misclick class
  D2's passthrough design exists to prevent (a leader continuation that isn't
  a real binding is a safe no-op, never a forwarded raw key).
- **IME / DE reservations** — `Ctrl+Shift+U` (IBus Unicode input, Linux),
  `Ctrl+Space` as an OS-level toggle (macOS input-source switch, Windows CJK
  IME) — this is exactly the leader key itself, hence the documented fallback
  keys and per-OS pre-flight below, not an outright ban.
- **Readline families** — `Ctrl+A/E` (line start/end), `Ctrl+K/U` (kill to
  end/start), `Ctrl+W` (kill word), `Ctrl+R` (reverse search), `Ctrl+L` (clear
  screen), `Ctrl+P/N` (history). These are muscle memory for anything backed
  by GNU readline or a readline-alike (shells, many CLI prompts) — nergal only
  claims a member of this family when it's also mirroring an OS/browser
  convention (e.g. `Ctrl+L` inside the browser panel's own URL bar).
- **CLI leaders** — whatever prefix/editing keys the *currently installed*
  agent CLIs claim for themselves (history navigation, external-editor
  invocation, their own internal command palettes, etc.). This class is
  intentionally under-specified here: it's exactly the set that changes
  release to release, which is why re-verification is event-driven (see
  Provenance) instead of a standing per-key list in this file.

## Deliberate shadows + mitigations

Nergal's default (`keyboard_ownership: "nergal"`) claims `Ctrl+B/W/S/L`,
`Alt+↑/↓` and `Ctrl+Enter` (fullscreen terminal) globally, shadowing native
agent bindings (background-command, delete-word, save-session, clear-line,
scroll-history, and Claude Code's send-now since v2.1.275 — whatever the active
CLI does with them). Send-now stays reachable through `leader .` then
`Ctrl+Enter`, or CC's own `Ctrl+X Ctrl+S`. Two mitigations, layered:

1. **`leader .` passthrough** — the next keystroke after `leader .` forwards
   verbatim to the active PTY, so a shadowed agent shortcut is always at most
   one extra prefix away. This is the general-purpose escape hatch and works
   regardless of ownership mode.
2. **`keyboard_ownership: "agent"`** — flips the small `scope: "app"` set to
   yield to the PTY whenever the terminal/quake zone holds focus, recovering
   native muscle memory for users who lean on those specific agent bindings
   constantly. Toggle in Settings → Keymap; see `docs/patterns.md` §1.4.

## Mode-independent exceptions

Two chord groups stay fixed regardless of `keyboard_ownership` — they aren't
part of the nergal/agent dispute this switch arbitrates:

- **Quake tab-management overrides** — `Ctrl+W` / `Ctrl+Shift+T` while a quake
  shell holds focus are hardcoded nergal-owned in both modes: a quake shell is
  nergal UI hosting a plain shell, not the agent CLI, so the ownership switch
  (which is about the *agent* dispute) doesn't apply. The shell's own readline
  `Ctrl+W` is still reachable via `leader .`.
- **Browser-iframe Tauri reserved set** — `Ctrl+T/W/Tab/Shift+Tab`, `F5`,
  `Ctrl+R`, and `Ctrl+Shift+0` (`browser:toggle-mode`) are registered as
  OS-level globals in `src-tauri/src/browser.rs` (`RESERVED_SHORTCUTS`) so
  they fire even while a cross-origin `<iframe>` traps keyboard focus —
  ordinary registry shortcuts (leader included) go silent there. `Ctrl+Shift+0`
  is a deliberate survivor here even though the *global* zen shortcut moved to
  `leader 0`: it's a different mechanism (OS-level bypass, iframe-scoped) kept
  alive specifically because the registry can't reach the iframe.

## Leader mechanics summary

- **Two discrete key-presses**, not a held chord: `Ctrl+Space` then a second
  key. 2000 ms timeout; `Esc` or re-tapping `Ctrl+Space` cancels.
- **Which-key popover** mounts after a 150 ms hesitation (derived live from
  the resolved registry, grouped by family) — fluent users resolve the chord
  before it ever appears.
- **Continuation resolution**: a plain key fires its continuation;
  `Ctrl+<continuation-letter>` is tolerated as the same continuation (sloppy
  chording — VS Code users often keep Ctrl held from the leader press);
  `Ctrl+<anything else>` is a safe no-op with a which-key hint — it never
  reaches the PTY. `leader .` enters **raw mode**: the next keystroke forwards
  verbatim to the active PTY (zone-aware: session terminal or quake shell).
- **Rebindable with declared bans** (post-walk revision 2026-07-03): `leader`
  and `focus-terminal` are ordinary rebindable rows in Settings → Keymap
  (they were UI-locked in the original design; the user opted for rebind +
  validation instead). Protection comes from `RESERVED_COMBOS` in
  `src/lib/keymap.ts` — a declared list of OS/DE-owned combos (IBus unicode
  input, GNOME open-terminal / lock-screen / workspace-switch) that
  `validateCombo` rejects with the reservation as the reason. Still locked:
  `command-palette` and `session-1..9`. Documented leader fallback: `ctrl+.`.
- **Focus terminal key label**: the binding targets the physical key right of
  L (`code: Semicolon`) — Ñ on Spanish/LA layouts, `;` on most others. Where
  the Keyboard API exists (Chromium-based webviews) the label adapts via
  `initKeyboardLayoutLabels()`; WebKitGTK lacks the API, so Linux shows the
  Ñ default. The match is by `event.code` either way, so the label is purely
  cosmetic.

## Per-OS leader pre-flight

`Ctrl+Space` is not universally free:

- **macOS**: default input-source-switch shortcut (System Settings → Keyboard
  → Keyboard Shortcuts → Input Sources). Disable it there, or rebind the
  leader row in Settings → Keymap (fallback: `ctrl+.`).
- **Windows**: common CJK IME toggle (Microsoft Pinyin, Japanese IME, …).
  Same fallback applies.
- **Linux / IBus**: verified free on the dev machine 2026-07-02 — GNOME/IBus
  default to `Super+Space` for input-source switching, not `Ctrl+Space`.

## Provenance

Audited 2026-07-02 against the four installed agent CLIs: Claude Code 2.1.199,
Codex 0.142.0, OpenCode 1.17.9, pi 0.73.1. The full per-CLI keybinding
inventory (the part that actually rots release to release) lives in the vault
— `Obsidian23/Projects/nergal/Shortcuts restructure proposal.md`, Apéndice B —
not here. Re-verification is event-driven, not scheduled: the CC
release-review sweep protocol (`project_cc_release_review.md` in Claude's
memory) gains a "new keybindings?" checklist item, so a per-key recheck only
happens when a CLI release actually changes something, and `validateCombo` +
this file's never-bind classes are what a new binding gets checked against at
that moment.
