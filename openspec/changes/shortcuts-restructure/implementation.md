# Implementation plan — shortcuts restructure v2

## Verified codebase facts (do not re-assume)

Verified against disk 2026-07-03:

- **Registry** — `src/stores/shortcuts.ts`
  - `ShortcutAction` interface at `src/stores/shortcuts.ts:46` (`id/label/keys/category/keywords/handler` — no `scope` field yet).
  - `shortcutRegistryAtom` at `:424`; `resolvedShortcutsAtom` (override-applied) at `:839`; `keymapCaptureActiveAtom` at `:831`; `keymapOverridesAtom` at `:834` reads `configAtom.keymap_overrides`.
  - `numericTargetWorkspace()` at `:109` already prefers the sidebar-focused workspace (`focusedWorkspaceIdAtom`, set by jump-to-project) — the `Ctrl+1..9` consolidation is behavior-preserving.
  - `switchToSessionInFocused()` at `:140` + `focused-session-1..9` entries at `:469-477` → to remove.
  - `project-1..9` entries (`ctrl+alt+1..9`) at `:479-487` → rebind to `ctrl+shift+1..9`.
  - Entries to move to the leader layer: `toggle-notifications` `:455` (`leader h`), `new-session` `:478` (`leader n`), `add-workspace` `:488` (`leader shift+w`), `open-browser` `:503` (`leader b`), `toggle-ports` `:447` (`leader o`), `open-clickup` `:526` (`leader c`), `toggle-activity` `:547` (`leader a`), `open-linear` `:548` (`leader l`), `open-crosssession` `:560` (`leader x`), `toggle-scratchpad` `:572` (`leader s`), `obsidian-quick-capture` `:573` (`leader q`), `obsidian-vault-search` `:597` (`leader v`), `toggle-annotations` `:619` (`leader d`), `expand-zen` `:675` (`leader 0`), `open-ide` `:709` (`leader e`), `rename-branch` `:769` (`leader r`), `clear-completed-tasks` `:772` (`leader t`), `complete-merge` `:782` (`leader m`), `push-session` `:806` (`leader p`).
  - `close-tab` `:496` stays `ctrl+w` but gains `scope: "app"`; a new `leader w` entry reuses `closeCurrentTab`'s session-soft-close branch (`softCloseSessionAction`).
  - `ship-session` `:736` → `ctrl+shift+enter`. `toggle-sidebar` `:426` stays `ctrl+b` + `scope: "app"`. `toggle-right-panel` `:427` keeps its key. `open-obsidian-finder` `:504` → `ctrl+shift+o` (was `ctrl+shift+q`, freed). `save-file` `:493`, `browser-focus-url` `:541`, `nav-up`/`nav-down` `:456-457` gain `scope: "app"`.
  - `navigateItems()` at `:281` returns early in terminal zone but the dispatcher still `preventDefault`s — the app-scope gate must fix this (return-before-claim).
- **Dispatcher** — `src/hooks/useKeyboardShortcuts.ts`
  - Single window-capture listener registered at `:238`. Order today: keymap-capture bail `:40` → Tab forwarding via `terminalService.sendSpecialKeyToActive` `:49-83` → quake Ctrl+W/Ctrl+Shift+T overrides `:88-105` → hardcoded Ctrl+K `:108` → scratchpad hijack `:120-151` → palette-open bail `:163` → dialog guard `:171` → zen alt+arrow guard `:180` → quake dual-match `:208-217` → registry loop `:219-235` (parseKeys + exact modifier/code match, `preventDefault` + handler).
  - PTY forwarding primitive already exists: `terminalService.sendSpecialKeyToActive(code, key, {ctrl?, shift?, alt?}, region)` (`terminalService.ts:477` — note the order: CODE first, then key; full modifier support, which is exactly what raw-mode forwarding needs). Used for Tab at `:55` and `:65`, quake variant with region `"quake"`.
  - Browser panel Tauri-level `RESERVED_SHORTCUTS` (`src-tauri/src/browser.rs:31-43`): `ctrl+t/w/tab/shift+tab/r`, `f5`, and `ctrl+shift+0 → browser:toggle-mode`; registered only while the embedded iframe holds focus (`useKeyboardShortcuts.ts:153-161`) — registry shortcuts (leader included) cannot fire there. `ctrl+shift+0` stays as an iframe-scoped exception (no Rust change).
- **Keymap lib** — `src/lib/keymap.ts`
  - `KEY_TO_CODE` `:7-31` (no `space` token yet; `ñ→Semicolon` at `:29`); `parseKeys` `:46`; `comboSignature` `:63`; `eventToKeys` `:74`; `formatKeyParts` `:88`; `LOCKED_SHORTCUT_IDS` `:113` (`command-palette`, `focus-terminal`, `session-1..9`); `RESERVED_SIGNATURES` `:122` (ctrl+shift+u); `validateCombo` `:137` (requires ctrl||alt at `:146`).
- **Keymap UI** — `src/components/settings/KeymapSection.tsx`
  - Groups by `category` via `CATEGORY_ORDER` `:15`; `KeyCombo` chip renderer `:23`; capture effect `:53-90` records single keydown via `eventToKeys` + `validateCombo`; override write-back to `config.keymap_overrides` `:75-81` (match-default clears); `resetAll` `:100`; locked rows render a `Lock` icon `:178-184`.
- **Palette** — `src/components/command/CommandPalette.tsx` renders badges via `formatKeyParts` (only other consumer besides KeymapSection).
- **Provider status** — popover state is local to `src/components/layout/StatusBar.tsx` (`openProvider`, fetch at `:571` via `get_provider_status_detail`). No registry entry, no palette entry today.
- **Config** — `keymap_overrides` lives on the frontend `Config` type consumed by `configAtom` (`src/stores/config.ts`); `keyboard_ownership` must be added there + to the Rust `Config` struct default (serde) — check `src-tauri/src/config.rs` for the field default pattern; frontend-owned ⇒ do NOT add to `BACKEND_OWNED_CONFIG_KEYS`.
- **Docs** — `docs/patterns.md` §1 documents the current modifier taxonomy (Ctrl+Shift / Ctrl+Alt tiers) — needs rewrite; §5.4 documents window-capture interception; §8 bare-letter verbs unaffected.
- **Spec staleness** — `openspec/specs/keyboard-shortcuts/spec.md` still references xterm.js, `Ctrl+Shift+M/C`, `ctrl+alt+v`: the delta re-baselines those requirements.

## Execution order

### Phase 1 — keymap primitives (`src/lib/keymap.ts`)
1. Add `space: "Space"` to `KEY_TO_CODE`; `formatKeyParts` renders `space` → `Space`.
2. Chord support: `parseKeys` (and a new `parseChord`) accept `"leader <combo>"`; `comboSignature` emits namespaced signatures (`L:<sig>` vs global `<sig>`); `eventToKeys` unchanged (single-step capture) — the two-step recorder composes chords in KeymapSection.
3. `LOCKED_SHORTCUT_IDS`: add `leader`, remove nothing else yet (`focused-session-*` ids disappear with their entries).
4. `validateCombo`: leader-namespace continuations skip the ctrl/alt requirement; collision check partitions by namespace; keep RESERVED check global.

### Phase 2 — registry (`src/stores/shortcuts.ts`)
5. Add `scope?: "global" | "app"` and `group?: string` (which-key family: surfaces/git/vault/session/terminal — the popover derives its grouping from this field, never from a popover-local list) to `ShortcutAction`; new `leader` registry entry (`keys: "ctrl+space"`, UI-locked, no-op handler — the dispatcher owns the state machine, the entry exists for palette/keymap visibility). `resolvedShortcutsAtom` (`shortcuts.ts:839-845`) must carve `leader` out of the ignore-overrides-for-locked-ids rule (design D9).
6. Apply the full remap table (facts above): leader-layer keys, `ctrl+shift+1..9` projects, remove `focused-session-*` + `switchToSessionInFocused`, ship → `ctrl+shift+enter`, scope tags on `toggle-sidebar`/`close-tab`/`save-file`/`browser-focus-url`/`nav-up`/`nav-down`.
7. New `leader w` (close session) entry calling `softCloseSessionAction`; new palette-only entry `provider-status` (`keys: ""`) dispatching `nergal:open-provider-status`.
8. `resolvedShortcutsAtom`: filter/ignore overrides for removed ids (belt) — actual cleanup in Phase 5.

### Phase 3 — dispatcher (`src/hooks/useKeyboardShortcuts.ts`)
9. Leader state machine module (pending flag + deadline + mode `awaiting-continuation | raw`), with the ordering from design D9 (iprev R1-F1): **resolution runs immediately after the keymap-capture bail (`:40`) and BEFORE the Tab-forwarding block (`:49`), the quake overrides (`:88`), the hardcoded `Ctrl+K` (`:108`) and the scratchpad hijack (`:120`)** — while pending, the machine owns every keydown (raw-mode `Ctrl+K` must reach the PTY, not the palette; Tab must not be forwarded by the Tab block). **Activation** (the initial `Ctrl+Space` match) lives with the registry loop, after the palette/dialog/zen guards; a dialog opening mid-pending self-cancels the state. Resolve per D2 semantics (plain / ctrl-tolerant continuation / `.` raw / Esc / re-tap / timeout via `setTimeout` cleanup).
10. Raw-mode forwarding through `terminalService.sendSpecialKeyToActive` respecting focus zone (quake vs terminal).
11. App-scope gate: registry loop consults `scope` + `keyboard_ownership` + `focusZoneAtom`; on decline, `continue` WITHOUT `preventDefault`. Audit that `nav-up`/`nav-down` no longer swallow in terminal.
12. Status-bar breadcrumb atom (`leaderPendingAtom`) exported for StatusBar.

### Phase 4 — which-key + StatusBar + palette
13. New `WhichKeyPopover` component (compact palette style, `design.md` §3.9 tokens): renders from `resolvedShortcutsAtom` filtered to leader chords, grouped (surfaces/git/vault/session/terminal `.`); 150 ms delayed mount; positioned above the status bar.
14. StatusBar: `Ctrl+Space …` breadcrumb while pending; listener for `nergal:open-provider-status`.
15. CommandPalette badge rendering for chords + no-badge for `keys: ""`.

### Phase 5 — keymap UI + config + migration
16. `Config` type + Rust `Config` struct: `keyboard_ownership` (default `"nergal"`), NOT in `BACKEND_OWNED_CONFIG_KEYS`; Settings → Keymap toggle with one-line mode explanations.
17. KeymapSection: layer grouping (Core/Surfaces/Leader/App-scope — derive layer from keys+scope), two-step recorder (capture prefix, then continuation; writing `"leader x"` overrides), chord chips.
18. Startup override cleanup (two rules, one pass): drop overrides for ids no longer in the registry, AND drop surviving overrides whose signature now collides with a NEW default (first-match-wins would silently shadow e.g. Ship on `ctrl+shift+enter`); one-time notice in Settings → Keymap listing every dropped entry and why.

### Phase 6 — hints sweep + docs
19. Grep for stale combo strings (`Ctrl+Shift+Y`, `Ctrl+Alt+`, `Ctrl+Shift+1`…) across `src/` — tooltips, empty-states, footers with `Kbd` chips; update to effective-binding rendering where possible (prefer reading from the registry over literals).
20. `docs/patterns.md` §1 rewrite (4-layer taxonomy); new `docs/shortcuts.md` (D8 scope); CLAUDE.md TOC pointer.
21. Amend `openspec/changes/setup-wizard/`: ownership-choice step (proposal bullet + task; spec delta if its wizard-steps requirement enumerates steps).

## Edge cases

- **`Ctrl+Shift+Enter` in the git-panel commit textarea**: the existing spec requirement *Ship-Enter contextual shortcut (git panel textarea)* binds the SAME combo locally (Ship using the textarea message; empty message → global dialog). The window-capture global fires before the textarea's bubble handler, so the dispatcher must defer to the local handler when the target is that textarea (same pattern as the `fullscreen-terminal` / `inNonTerminalField` guard at `useKeyboardShortcuts.ts:229`). Semantics converge deliberately: one key, context-sensitive payload.
- Leader pressed while palette/dialog/zen open → activation sits after those guards (must not activate under a dialog); resolution runs early per D9, and a dialog opening mid-pending self-cancels the state.
- **Quake overrides during pending**: resolution-first ordering means quake `Ctrl+W`/`Ctrl+Shift+T` never fire while the leader is pending (the machine owns the keydown). Outside pending they stay nergal-owned in BOTH ownership modes (deliberate exception, spec *Shortcut scopes*).
- **Browser iframe focus**: leader chords structurally cannot fire while the cross-origin iframe holds focus (iframe traps keydown); the Tauri reserved set including `ctrl+shift+0` toggle-mode is the iframe-scoped keymap — document in `docs/shortcuts.md`, no Rust change.
- Leader while keymap recorder active → `keymapCaptureActiveAtom` bail already first; recorder owns the keystrokes.
- Raw mode + special keys (arrows, Enter, Tab) → forward via the same special-key path as Tab forwarding.
- Continuation keys with Shift (`leader shift+w`) → match on `e.shiftKey` + code; plain `w` and `shift+w` are distinct continuations.
- `Ctrl+Space` on macOS/Windows reserved by IM → per-OS pre-flight note in `docs/shortcuts.md`; fallback rebind path works via the resolved D9 semantics: `leader` is UI-locked but `keymap_overrides["leader"]` is honored (the sole locked-id carve-out).
- Quake overlay open: leader continuations that target panels behave as today (quake is an overlay, not a zone in `getVisibleZones`).

## Per-phase risk

| Phase | Risk | Mitigation |
|---|---|---|
| 1-2 | Signature/namespace regressions breaking existing overrides | unit-test `comboSignature`/`validateCombo` both namespaces; belt in `resolvedShortcutsAtom` |
| 3 | Swallowing keystrokes meant for the PTY (worst failure class) | leader consumes ONLY while pending; timeout hard-clears; manual PTY typing walk |
| 4 | Which-key drift vs registry | render exclusively from `resolvedShortcutsAtom` |
| 5 | Config round-trip drops the new key | follow [[feedback_frontend_config_stale]]: frontend-owned, verify `save_config` retains it |
| 6 | Stale hints survive | grep sweep with a checklist in tasks.md, not memory |
