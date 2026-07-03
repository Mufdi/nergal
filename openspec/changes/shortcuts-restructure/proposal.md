# Shortcuts restructure v2 — leader key + 4-layer keymap

## Why

Nergal's dispatcher intercepts keydown at window-capture *before* the PTY, so every global shortcut permanently steals that combination from the agent CLI underneath. An audit of the four installed agent CLIs (Claude Code 2.1.199, Codex 0.142.0, OpenCode 1.17.9, pi 0.73.1 — 2026-07-02) showed that bare `Ctrl+letter` is almost fully occupied by the agents (readline + CC features) and `Ctrl+Alt+letter` collides head-on with OpenCode's message-scroll keys. Today nergal silently shadows ~12 agent bindings (`Ctrl+B/L/N/W/S/K`, `Alt+↑/↓`, …), the 73-entry flat registry has run out of mnemonic letters, and there is no mechanism to reach a shadowed agent shortcut at all. The full design rationale lives in the vault working doc (`Obsidian23/Projects/nergal/Shortcuts restructure proposal.md`, v2 — source of design truth for this change).

## What Changes

- **New leader key `Ctrl+Space`** (physical `code: "Space"`, locked, remappable fallbacks `Ctrl+{` / `Ctrl+.`): a tmux/vim-style prefix opening a second-key namespace for the long tail of shortcuts (integrations, git actions, vault, misc). Two discrete presses; 2 s timeout; `Esc`/re-tap cancels.
- **Which-key popover**: on leader press, a compact popover (derived from the registry, never hardcoded) lists valid continuations grouped by family, appearing after a ~150 ms delay; a `Ctrl+Space …` breadcrumb shows in the status bar while the leader is pending.
- **Explicit passthrough (send-to-terminal)**: `leader` → `.` forwards the *next* keystroke verbatim to the active PTY, so every agent shortcut remains reachable even when nergal shadows it (worst case costs a prefix). With the leader pending, `Ctrl+<continuation-letter>` is tolerated as the continuation (sloppy chording); `Ctrl+<anything else>` is a safe no-op with a which-key hint — never an accidental write to the PTY.
- **Keyboard ownership switch**: new frontend-owned config `keyboard_ownership: "nergal" | "agent"` (default `"nergal"`). The binding map is identical in both modes; the switch only flips who wins the disputed keys: `nergal` claims everything globally (agent shortcuts reachable via passthrough); `agent` activates an app-scope gate so `Ctrl+B/W/S/L` and `Alt+↑/↓` pass through to the PTY when focus is in the terminal.
- **Registry gains `scope: "global" | "app"`** and chord syntax in `keys` (`"leader n"`, `"leader shift+w"`); palette-only entries supported via `keys: ""`.
- **Remapped bindings** (**BREAKING** for existing muscle memory and for saved `keymap_overrides`):
  - `Ctrl+Shift+1..9` = Jump to Project N (was `Ctrl+Alt+1..9`); `Ctrl+1..9` absorbs the focused-workspace session switch (the `focused-session-*` entries are removed — `numericTargetWorkspace` already implements the preference). The `Ctrl+Alt` tier dies entirely.
  - Ship = `Ctrl+Shift+Enter` (was `Ctrl+Shift+Y`).
  - Long-tail moves to the leader layer: Activity `a`, Browser `b`, ClickUp `c`, Linear `l`, Ports `o`, Cross-session `x`, Scratchpad `s`, Annotations drawer `d`, Notification history `h`, Zen `0`, Push `p`, Complete merge `m`, Rename branch `r`, Quick capture `q`, Vault search `v`, New session `n`, Close session `w`, Add workspace `Shift+W`, Open in IDE `e`, Clear completed tasks `t`.
  - Obsidian panel = `Ctrl+Shift+O` (was `Ctrl+Shift+Q`) — promoted to Surfaces with its true mnemonic; the ports popover takes `leader o`.
  - Kept as-is: `Ctrl+B` sidebar (becomes `scope: "app"`), `Ctrl+Shift+B` right panel, `Ctrl+K` palette, `Ctrl+Enter` fullscreen, `Ctrl+}` quake, `Ctrl+Ñ` focus terminal, `Ctrl+Tab`/`Ctrl+Shift+Tab`, `Ctrl+,`, surfaces `Ctrl+Shift+P/G/D/F/S/K/T/H/R`.
- **Provider-status popover** (Claude/OpenAI status) gains a palette-only entry (today it is click-only in the status bar and absent from the palette). No dedicated key.
- **App-scope entries** (`Ctrl+B/W/S/L`, `Alt+↑/↓`): in `agent` mode they only claim the key when `focusZoneAtom` is not terminal/quake — otherwise the event reaches the PTY (real passthrough, no swallowed no-ops).
- **Keymap editor upgrades**: grouping by layer (Core / Surfaces / Leader / App-scope), a two-step recorder for chords, `LOCKED_SHORTCUT_IDS` gains `leader` and drops the removed `focused-session-*` ids; one-time cleanup of `keymap_overrides` that reference removed ids or whose combo collides with a new default (with a notice listing what was dropped and why).
- **Hints sweep**: every visible `Kbd` chip, tooltip, empty-state and doc (`docs/patterns.md` §1 taxonomy) updated to the new map; new `docs/shortcuts.md` recording layer conventions, the never-bind list, deliberate shadows, and the provenance stamp (audit date + CLI versions).
- **Onboarding hook**: the ownership question is added as a step of the (not yet implemented) `setup-wizard` change — that change's artifacts are amended, not duplicated here.

## Capabilities

### New Capabilities

<!-- None. The leader layer, which-key, passthrough and ownership switch are requirement-level changes to the existing keyboard-shortcuts capability. -->

### Modified Capabilities

- `keyboard-shortcuts`: layer model (Core / Surfaces / Leader / App-scope), leader state machine + which-key + passthrough, ownership switch, full binding remap, palette-only entries, keymap-editor chord recording. This spec is also partially stale (references xterm.js, removed `Ctrl+Shift+M/C` bindings, `ctrl+alt+v` vault search) — the delta re-baselines the affected requirements to implemented reality.
- `command-palette`: key badges must render chords (`Ctrl+Space` `N`) and tolerate entries with no binding (`keys: ""`).

## Impact

- **Frontend**: `src/stores/shortcuts.ts` (registry: scope field, chord keys, remaps, removals, palette-only entries), `src/hooks/useKeyboardShortcuts.ts` (leader state machine, passthrough, app-scope gate, no-op audit), `src/lib/keymap.ts` (chord parsing/signatures/format, `space` token, LOCKED set, validation rules for leader-namespace), new which-key popover component, `src/components/settings/KeymapSection.tsx` (layer grouping, two-step recorder), status-bar breadcrumb, command palette badge rendering, config types + `keymap_overrides` migration.
- **Config**: new `keyboard_ownership` key (frontend-owned in `config.json` — must NOT be added to `BACKEND_OWNED_CONFIG_KEYS`).
- **Docs**: `docs/patterns.md` §1 rewrite, new `docs/shortcuts.md`, CLAUDE.md pointer.
- **Cross-change**: `openspec/changes/setup-wizard/` amended with the ownership onboarding step.
- **Backend impact minimal**: one additive serde field on the Rust `Config` struct (`keyboard_ownership`); no other Rust changes (the browser panel's Tauri-level reserved chords stay as-is as iframe-scoped exceptions), no SQLite schema changes, no new dependencies. Terminal-side key forwarding (`terminalService.sendSpecialKeyToActive`) is reused as-is for passthrough.
- **User-facing**: existing muscle memory for the moved bindings breaks (deliberate, user-approved redesign); a one-time notice surfaces cleaned-up overrides.
