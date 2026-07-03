# Tasks — shortcuts restructure v2

Read `implementation.md` first — it carries the verified line refs and the remap table.

## 1. Keymap primitives (`src/lib/keymap.ts`)

- [x] 1.1 Add `space` token to `KEY_TO_CODE` (`"Space"`) and `formatKeyParts` (`Space`); keep `event.code`-only matching (WebKitGTK).
- [x] 1.2 Chord parsing: accept `keys: "leader <combo>"` (e.g. `"leader n"`, `"leader shift+w"`); namespaced `comboSignature` (leader vs global) so collisions are checked per namespace.
- [x] 1.3 `validateCombo`: leader continuations do not require Ctrl/Alt; global rules unchanged; reserved list still global.
- [x] 1.4 `LOCKED_SHORTCUT_IDS`: add `leader`; final set = `command-palette`, `focus-terminal`, `leader`, `session-1..9`. Lock semantics per design D9: UI-only lock for `leader` — `resolvedShortcutsAtom` honors `keymap_overrides["leader"]` (carve-out from the ignore-locked rule at `shortcuts.ts:839-845`).
- [x] 1.5 Unit tests for chord parse/signature/validate (both namespaces, shift-continuations, `keys: ""`).

## 2. Registry (`src/stores/shortcuts.ts`)

- [x] 2.1 Add `scope?: "global" | "app"` (default global) and `group?: string` (which-key family) to `ShortcutAction`; set `group` on every leader-layer entry.
- [x] 2.2 Add the `leader` entry (`ctrl+space`, category navigation, palette-visible, no-op handler — dispatcher owns the state machine).
- [x] 2.3 Apply the remap table from implementation.md: 19 leader-layer moves, `ship-session` → `ctrl+shift+enter`, `project-1..9` → `ctrl+shift+1..9`, zen → `leader 0`.
- [x] 2.3b `ship-session` global must defer to the git-panel commit textarea's local `Ctrl+Shift+Enter` handler (see implementation.md edge cases — spec req. *Ship-Enter contextual shortcut*).
- [x] 2.4 Remove `focused-session-1..9` entries and `switchToSessionInFocused`; verify jump-to-project leaves focus in sidebar so `numericTargetWorkspace` covers the flow (add the focus assertion if missing).
- [x] 2.5 Tag app-scope entries: `toggle-sidebar`, `close-tab`, `save-file`, `browser-focus-url`, `nav-up`, `nav-down`.
- [x] 2.6 New `leader w` close-session entry (soft-close via `softCloseSessionAction`); new palette-only `provider-status` entry (`keys: ""`) dispatching `nergal:open-provider-status`.

## 3. Dispatcher (`src/hooks/useKeyboardShortcuts.ts`)

- [x] 3.1 Leader state machine (pending + deadline + `awaiting-continuation | raw` mode) with D9 split ordering: **resolution immediately after the keymap-capture bail** (before Tab forwarding / quake overrides / hardcoded Ctrl+K / scratchpad hijack — while pending the machine owns every keydown), **activation** with the registry loop after the dialog/palette/zen guards; dialog opening mid-pending self-cancels; `Esc`/re-tap/timeout(2000 ms) cancel cleanly.
- [x] 3.2 Continuation resolution per design D2: plain key; `Ctrl+<continuation>` tolerance; `Ctrl+<other>` safe no-op + hint state; `.` enters raw mode.
- [x] 3.3 Raw-mode forwarding via `terminalService.sendSpecialKeyToActive`, zone-aware (quake vs session terminal); support special keys (arrows/Enter/Tab).
- [x] 3.4 App-scope gate: consult `scope` × `keyboard_ownership` × `focusZoneAtom`; on decline `continue` WITHOUT `preventDefault`. Verify `nav-up`/`nav-down` no longer swallow events in terminal zone; pi recovers `Alt+↑`.
- [x] 3.5 Export `leaderPendingAtom` (or equivalent) for StatusBar + which-key.

## 4. Which-key + surfaces

- [x] 4.1 `WhichKeyPopover` component: derived from `resolvedShortcutsAtom` (leader chords only), grouped (surfaces / git / vault / session / send-to-terminal `.`), 150 ms delayed mount, hides on resolve/cancel. Style per `docs/design.md` (compact palette).
- [x] 4.2 StatusBar breadcrumb `Ctrl+Space …` while pending; hint variant when a non-continuation Ctrl-combo was pressed.
- [x] 4.3 StatusBar listener for `nergal:open-provider-status` opening the provider popover.
- [x] 4.4 CommandPalette: chord badges (`Ctrl+Space` + key) and badge-less rows for `keys: ""`.

## 5. Config + keymap UI + migration

- [x] 5.1 `keyboard_ownership: "nergal" | "agent"` on frontend `Config` type + Rust `Config` struct (serde default `"nergal"`); frontend-owned — do NOT add to `BACKEND_OWNED_CONFIG_KEYS`; verify `save_config` round-trip retains it.
- [x] 5.2 Settings → Keymap: ownership toggle with one-line explanation per mode.
- [x] 5.3 KeymapSection: group by layer (Core / Surfaces / Leader / App-scope); two-step recorder for chords; chord `Kbd` chips.
- [x] 5.4 Startup cleanup of `keymap_overrides`: drop removed-id overrides AND surviving overrides whose signature collides with a new default (Ship `ctrl+shift+enter`, Obsidian `ctrl+shift+o`, projects `ctrl+shift+1..9`, …); one-time notice in Settings → Keymap listing every drop and why.

## 6. Hints sweep + docs + cross-change

- [x] 6.1 Grep sweep for stale combo literals (`Ctrl+Shift+Y`, `Ctrl+Alt+*`, `ctrl+alt+`, `Ctrl+Shift+1`) across `src/`; update every tooltip / empty-state / footer `Kbd` chip; prefer rendering from the resolved registry over string literals.
- [x] 6.2 Rewrite `docs/patterns.md` §1 taxonomy (4 layers; `Ctrl+Alt` tier removed; leader + app-scope added).
- [x] 6.3 New `docs/shortcuts.md`: layer conventions + allocation rule (new features born in the leader), never-bind list by stable classes, deliberate shadows + mitigations, the two mode-independent exceptions (quake tab-management overrides; browser-iframe Tauri reserved set incl. `ctrl+shift+0` toggle-mode), provenance stamp (2026-07-02; CC 2.1.199 / Codex 0.142.0 / OpenCode 1.17.9 / pi 0.73.1), per-OS leader pre-flight notes. CLAUDE.md TOC pointer.
- [x] 6.4 Amend `openspec/changes/setup-wizard/`: ownership-choice wizard step (proposal bullet + task) referencing this change. *(Done at proposal time — verify still consistent at implementation.)*

## 7. Verification

- [x] 7.1 `npx tsc --noEmit` clean.
- [x] 7.2 Frontend unit tests (keymap chord suite) green; `cd src-tauri && cargo check` (config struct) clean.
- [ ] 7.3 Manual walk — leader: `Ctrl+Space` `c` opens ClickUp from terminal focus; timeout/Esc/re-tap cancel; which-key appears on hesitation and reflects a remap; breadcrumb visible.
- [ ] 7.4 Manual walk — passthrough: `leader . Ctrl+K` reaches the PTY (CC kill-line); `leader Ctrl+Z` no-ops with hint (nothing reaches PTY); sloppy `Ctrl+Space Ctrl+p` fires Push.
- [ ] 7.5 Manual walk — ownership: default mode `Ctrl+W` in terminal soft-closes session; switch to `agent` → `Ctrl+W` reaches PTY (delete-word) and `Alt+↑` reaches pi; outside terminal `Ctrl+W` still closes panel tab.
- [ ] 7.6 Manual walk — digits: `Ctrl+Shift+3` then `Ctrl+2` lands on session 2 of project 3; `Ctrl+Shift+Enter` opens Ship from terminal.
- [ ] 7.7 Override migration: seed a `focused-session-2` override in config, start app, confirm cleanup + notice; remap a leader continuation and confirm palette + which-key reflect it.
