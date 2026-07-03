## ADDED Requirements

### Requirement: Leader key and pending state machine
The system SHALL provide a leader key bound to `Ctrl+Space` (physical `event.code === "Space"`, layout-independent). Pressing the leader SHALL enter a *pending* state that waits for one continuation keystroke. The interaction is two discrete presses (press and release the leader, then press one key) — no key needs to be held.

While the leader is pending:
- ALL keydown events SHALL be consumed by the dispatcher (nothing reaches the PTY) until the state resolves or cancels.
- The state SHALL cancel without side effects on `Esc`, on a second leader press (re-tap), or after a 2000 ms timeout.
- A valid continuation SHALL fire its registered action and clear the state.

The leader binding SHALL be locked in the keymap editor UI (listed in `LOCKED_SHORTCUT_IDS`: no Rebind button, protecting against accidental remaps of the structural prefix), **but** the override resolver SHALL honor a `keymap_overrides["leader"]` entry — this is the escape hatch for systems where the OS reserves `Ctrl+Space` (macOS input-source switch, CJK IME); documented fallbacks are `Ctrl+{` and `Ctrl+.`. This makes `leader` the one locked id whose override is not ignored.

#### Scenario: Leader override honored despite UI lock
- **WHEN** config contains `keymap_overrides: { "leader": "ctrl+." }`
- **THEN** the keymap editor shows the leader row locked, but `Ctrl+.` activates the leader and `Ctrl+Space` does not

#### Scenario: Leader then continuation fires the action
- **WHEN** the user presses and releases `Ctrl+Space`, then presses `p`
- **THEN** the Push action fires and the pending state clears

#### Scenario: Timeout cancels cleanly
- **WHEN** the user presses `Ctrl+Space` and presses nothing for 2000 ms
- **THEN** the pending state clears, no action fires, and no keystroke is sent to the PTY

#### Scenario: Esc cancels
- **WHEN** the leader is pending and the user presses `Esc`
- **THEN** the pending state clears and the `Esc` is NOT forwarded to the PTY

#### Scenario: Leader works from terminal focus
- **WHEN** focus is in the terminal and the user presses `Ctrl+Space` then `c`
- **THEN** the ClickUp panel toggles and neither keystroke reaches the PTY

### Requirement: Which-key popover
On leader press the system SHALL show a which-key popover listing the valid continuations with their labels, grouped by family (surfaces, git, vault, session, terminal). The popover:
- SHALL appear after a configurable delay (default 150 ms) so fluent users never see it.
- SHALL be derived from the resolved shortcut registry (labels + effective keys + a `group` metadata field on registry entries), never from a popover-local hardcoded list.
- SHALL disappear when the pending state resolves or cancels.

While the leader is pending, the status bar SHALL show a `Ctrl+Space …` breadcrumb.

#### Scenario: Hesitant user sees the menu
- **WHEN** the user presses `Ctrl+Space` and waits 200 ms
- **THEN** the popover is visible and lists every leader continuation with its label and key

#### Scenario: Fluent user never sees it
- **WHEN** the user presses `Ctrl+Space` then `c` within 150 ms
- **THEN** the ClickUp panel opens and the popover never rendered

#### Scenario: Popover reflects remaps
- **WHEN** a leader continuation has been remapped via keymap overrides
- **THEN** the popover shows the effective (overridden) key, not the default

### Requirement: Passthrough to the agent (send-to-terminal)
The system SHALL guarantee that every agent-CLI shortcut remains reachable even when nergal shadows it. With the leader pending:
- `.` SHALL enter *raw mode*: the next keystroke (any combination, including bare keys) is forwarded verbatim to the active PTY (respecting the active session and the quake zone), then the state clears. Raw mode shares the 2000 ms timeout.
- `Ctrl+<key>` where `<key>` is a registered continuation SHALL be tolerated as that continuation (sloppy chording — the user kept Ctrl held from the leader press).
- `Ctrl+<key>` where `<key>` is NOT a continuation SHALL be a safe no-op that keeps the which-key open with a hint pointing at `.` (send to terminal). It MUST NOT be forwarded to the PTY implicitly — an accidental `Ctrl+Z`/`Ctrl+D` must never reach the agent by mistake.

#### Scenario: Shadowed agent shortcut delivered via raw mode
- **WHEN** the user presses `Ctrl+Space`, then `.`, then `Ctrl+K`
- **THEN** `Ctrl+K` is written to the active PTY (Claude Code kill-to-EOL) and the command palette does NOT open

#### Scenario: Sloppy chording tolerated
- **WHEN** the user presses `Ctrl+Space` and, with Ctrl still held, presses `p` (producing `Ctrl+P`)
- **THEN** the Push action fires (same as plain `p`)

#### Scenario: Non-continuation Ctrl combo is safe
- **WHEN** the leader is pending and the user presses `Ctrl+Z`
- **THEN** nothing is sent to the PTY, no action fires, and the which-key shows the send-to-terminal hint

#### Scenario: Raw mode respects the quake zone
- **WHEN** focus is in the quake terminal and the user sends a combo via `leader` → `.`
- **THEN** the keystroke is forwarded to the quake shell's PTY, not the agent session

### Requirement: Keyboard ownership switch
The system SHALL provide a config setting `keyboard_ownership: "nergal" | "agent"` (default `"nergal"`), frontend-owned in `config.json` and editable from Settings → Keymap. The binding map SHALL be identical in both modes; the switch only decides who wins the disputed keys:

- **`nergal`**: every registry binding is claimed globally, including the app-scope entries. Agent shortcuts are reached via passthrough.
- **`agent`**: entries with `scope: "app"` only claim the key when the focus zone is not terminal/quake; with focus in the terminal the keydown reaches the PTY untouched.

#### Scenario: Default mode claims disputed keys globally
- **WHEN** `keyboard_ownership` is `"nergal"` (default), focus is in the terminal, and the user presses `Ctrl+W`
- **THEN** the session soft-closes (nergal behavior); the PTY does not receive the key

#### Scenario: Agent mode passes disputed keys through
- **WHEN** `keyboard_ownership` is `"agent"`, focus is in the terminal, and the user presses `Ctrl+W`
- **THEN** the keydown reaches the PTY (agent delete-word) and no nergal action fires

#### Scenario: Agent mode still claims app-scope keys outside the terminal
- **WHEN** `keyboard_ownership` is `"agent"`, focus is in the right panel, and the user presses `Ctrl+W`
- **THEN** the active panel tab closes

### Requirement: Shortcut scopes
Registry entries SHALL carry a `scope: "global" | "app"` field (default `"global"`). The app-scope set is: `Ctrl+B` (toggle sidebar), `Ctrl+W` (close tab), `Ctrl+S` (save file), `Ctrl+L` (focus browser URL bar), `Alt+↑/↓` (navigate items). When an app-scope entry declines to claim a key (agent mode + terminal focus), the dispatcher MUST return without `preventDefault` so the event genuinely reaches the PTY — handlers that would no-op MUST NOT swallow the event.

Two deliberate exceptions sit outside this gate and remain nergal-owned in BOTH ownership modes (documented in `docs/shortcuts.md`):
- the quake tab-management overrides (`Ctrl+W` close shell / `Ctrl+Shift+T` new shell while quake owns focus) — quake shell tabs are nergal UI, and their PTY hosts a plain shell, not the agent;
- the browser panel's Tauri-level reserved chords (`Ctrl+T/W/Tab/Shift+Tab/R/F5` and `Ctrl+Shift+0` toggle-mode), which fire only while the embedded cross-origin iframe holds focus — the iframe traps ordinary keydown, so registry shortcuts (including leader chords) structurally cannot fire there and these OS-level chords are the only escape hatches. `Ctrl+Shift+0` therefore survives *iframe-focus only* even though the global zen binding moves to `leader 0`.

#### Scenario: No swallowed no-ops
- **WHEN** `keyboard_ownership` is `"agent"`, focus is in the terminal, and the user presses `Alt+↑`
- **THEN** the event is not prevented and the PTY receives it (pi dequeue), instead of a silent nergal no-op

#### Scenario: Quake tab management is mode-independent
- **WHEN** `keyboard_ownership` is `"agent"`, focus is in the quake terminal, and the user presses `Ctrl+W`
- **THEN** the active quake shell tab closes (deliberate exception; the shell's delete-word remains reachable via `leader .`)

### Requirement: Palette-only registry entries
The registry SHALL support entries with `keys: ""` — actions with no key binding that still appear in the command palette (and can later be bound via the keymap editor). The provider-status popover (agent status: Claude/OpenAI) SHALL be exposed as a palette-only entry.

#### Scenario: Provider status reachable from the palette
- **WHEN** the user opens the command palette and types "status"
- **THEN** a "Provider status" entry appears without a key badge, and selecting it opens the provider-status popover

#### Scenario: Unbound entries never match keydown
- **WHEN** an entry has `keys: ""`
- **THEN** the dispatcher skips it for every keydown

### Requirement: Keymap editor supports the layer model
Settings → Keymap SHALL group shortcuts by layer (Core / Surfaces / Leader / App-scope), record two-step chords for leader continuations (first the prefix, then the continuation), and display chords as `Ctrl+Space` + key in `Kbd` chips. `LOCKED_SHORTCUT_IDS` SHALL contain `command-palette`, `focus-terminal`, `leader`, and `session-1..9`. Combo validation SHALL allow leader continuations without a Ctrl/Alt modifier (the prefix already isolates them from terminal typing) and SHALL check collisions within the leader namespace separately from the global namespace.

#### Scenario: Recording a leader continuation
- **WHEN** the user records a new binding for "ClickUp panel" and presses `Ctrl+Space` then `u`
- **THEN** the editor captures the chord `leader u` and validates it against other leader continuations only

#### Scenario: Bare-letter global rejected, bare-letter continuation accepted
- **WHEN** the user tries to bind a plain `g` as a global shortcut
- **THEN** validation rejects it (terminal typing interference)
- **WHEN** the user binds plain `g` as a leader continuation
- **THEN** validation accepts it

### Requirement: Keymap overrides migration
On startup after the restructure, `keymap_overrides` SHALL be cleaned in one pass:
- entries referencing removed shortcut ids (e.g. `focused-session-*`) SHALL be dropped;
- surviving overrides whose combo now collides (same signature, same namespace) with a NEW default binding SHALL also be dropped — under first-match-wins dispatch a pre-restructure override to e.g. `ctrl+shift+enter` or `ctrl+shift+o` (both free when it was validated) would otherwise silently shadow Ship / the Obsidian panel.

The user SHALL see a one-time notice in Settings → Keymap listing every dropped entry and why. Non-colliding overrides for surviving ids remain in effect over the new defaults.

#### Scenario: Stale override cleaned
- **WHEN** config contains an override for `focused-session-3` and the app starts after the restructure
- **THEN** the override is removed from config and the one-time notice mentions it

#### Scenario: Colliding override cleaned
- **WHEN** config contains `{"open-ide": "ctrl+shift+enter"}` (valid pre-restructure) and the app starts after the restructure
- **THEN** the override is dropped (it collides with the new Ship default), `open-ide` falls back to `leader e`, and the notice explains the collision

## MODIFIED Requirements

### Requirement: Terminal focus bypass
Keyboard ownership between nergal and the agent CLI SHALL be governed by the `keyboard_ownership` switch (see *Keyboard ownership switch*), not by a blanket terminal bypass. In both modes, keys not bound by nergal reach the PTY untouched, and `Ctrl+Ñ` (physical `Semicolon`) always focuses the terminal from any zone. The which-key/leader pending state is the only situation where unbound keys are withheld from the PTY.

#### Scenario: Unbound key reaches the PTY
- **WHEN** focus is in the terminal and the user presses `Ctrl+G` (unbound in nergal)
- **THEN** the keystroke reaches the PTY (agent external-editor)

#### Scenario: Ctrl+Ñ escapes to terminal from any zone
- **WHEN** focus is in any zone and the user presses `Ctrl+Ñ`
- **THEN** focus moves to the terminal

### Requirement: Navigation shortcuts
The system SHALL provide: `Ctrl+B` toggle sidebar (`scope: "app"`), `Ctrl+Shift+B` toggle right panel (global), `Alt+←/→` cycle focus zones (global), `Ctrl+Ñ` focus terminal (locked), `Ctrl+}` quake terminal (dual key/code match), `Ctrl+Enter` fullscreen terminal, `Ctrl+,` settings, `leader o` toggle ports popover, and `leader h` notification history.

#### Scenario: Toggle sidebar in default mode
- **WHEN** `keyboard_ownership` is `"nergal"` and the user presses `Ctrl+B` from any zone
- **THEN** the sidebar toggles

#### Scenario: Sidebar yields to the agent in agent mode
- **WHEN** `keyboard_ownership` is `"agent"`, focus is in the terminal, and the user presses `Ctrl+B`
- **THEN** the PTY receives `Ctrl+B` (Claude Code background command) and the sidebar does not toggle

#### Scenario: Ports popover via leader
- **WHEN** dev servers are detected and the user presses `Ctrl+Space` then `o`
- **THEN** the ports popover toggles

### Requirement: Session switching shortcuts
The system SHALL provide `Ctrl+1..9` to switch to session N (locked). The target workspace SHALL prefer the sidebar-focused workspace when the sidebar owns focus (existing `numericTargetWorkspace` behavior), falling back to the active session's workspace. The system SHALL provide `Ctrl+Shift+1..9` to jump to project/workspace N (was `Ctrl+Alt+1..9`); jumping SHALL leave focus in the sidebar so a following `Ctrl+digit` targets the jumped-to workspace. The `focused-session-*` entries (`Ctrl+Shift+1..9` old semantics) are removed as redundant.

#### Scenario: Jump to project then pick a session
- **WHEN** the user presses `Ctrl+Shift+3` (project 3) and then `Ctrl+2`
- **THEN** session 2 of project 3 becomes active

#### Scenario: Ctrl+digit from terminal focus
- **WHEN** focus is in the terminal and the user presses `Ctrl+2`
- **THEN** session 2 of the active workspace becomes active

### Requirement: Panel opening shortcuts
Daily surfaces SHALL keep `Ctrl+Shift+letter` (mnemonic = initial): `P` plan, `G` git, `D` diff, `F` files, `S` spec, `K` file picker, `T` reopen last closed, `H` toggle annotation mode, `R` revise/resolve/apply (contextual), `B` right panel, `O` Obsidian panel (was `Ctrl+Shift+Q`; keeps the `obsidianEnabledAtom` gate). Long-tail surfaces SHALL move to the leader layer: `leader a` activity drawer, `leader b` browser, `leader c` ClickUp, `leader l` Linear, `leader o` ports popover, `leader x` cross-session, `leader s` scratchpad, `leader d` annotations drawer, `leader 0` zen (expand active panel). The system MUST NOT bind `Ctrl+Shift+U` (IBus), `Ctrl+Shift+C` (terminal copy), or `Ctrl+Shift+A`/`Ctrl+Shift+E` (OpenCode select-line).

#### Scenario: Integration panel via leader
- **WHEN** the user presses `Ctrl+Space` then `l`
- **THEN** the Linear panel toggles (gated on API key as today)

#### Scenario: Zen via leader 0
- **WHEN** a panel is active and the user presses `Ctrl+Space` then `0`
- **THEN** the active panel expands to Zen (same contextual routing as the old `Ctrl+Shift+0`)

### Requirement: Action shortcuts
The system SHALL provide: `Ctrl+Shift+Enter` Ship (was `Ctrl+Shift+Y`), `leader p` push, `leader m` complete merge, `leader r` rename branch, `leader q` Obsidian quick capture, `leader v` vault search, `leader n` new session, `leader w` close session (soft-close), `leader Shift+W` add workspace, `leader e` open in IDE, `leader t` clear completed tasks. The stale `Ctrl+Shift+M` (merge modal) and `Ctrl+Shift+C` (commit modal) bindings no longer exist (merge/commit live in the Git panel; `Ctrl+Shift+C` is never-bind).

#### Scenario: Ship from any zone
- **WHEN** the user presses `Ctrl+Shift+Enter` with an active git-workspace session
- **THEN** the Ship preview dialog opens (same pre-checks as today: nothing-to-ship toast when empty)

#### Scenario: New session via leader
- **WHEN** the user presses `Ctrl+Space` then `n`
- **THEN** a new session flow starts (the agent CLI recovers bare `Ctrl+N` history-next)

### Requirement: Ship action shortcut (global)
The system SHALL bind `Ctrl+Shift+Enter` to the Ship action. From any focus zone, pressing it SHALL open the Ship preview dialog for the active session (with the existing nothing-to-ship pre-check). The registry entry keeps `id: "ship-session"`.

#### Scenario: Ship shortcut fires from terminal zone
- **WHEN** focus is in the terminal and the user presses `Ctrl+Shift+Enter`
- **THEN** the Ship preview dialog opens

#### Scenario: Ctrl+Enter neighbor misfire is recoverable
- **WHEN** the user intended `Ctrl+Enter` (fullscreen) but pressed `Ctrl+Shift+Enter`
- **THEN** the Ship dialog (or nothing-to-ship toast) appears and `Esc` dismisses it without side effects

### Requirement: Push action shortcut (global)
The system SHALL bind `leader p` to the explicit Push action (push-only, no commit, no PR), replacing `Ctrl+Alt+P`. Behavior is unchanged: push when there is work to push, toast otherwise.

#### Scenario: Push via leader
- **WHEN** the user presses `Ctrl+Space` then `p` with commits ahead
- **THEN** the system pushes and shows the result toast

### Requirement: Obsidian quick capture shortcut
The registry entry `obsidian-quick-capture` SHALL be bound to `leader q` (was `Ctrl+Alt+Q`), keeping the `obsidianEnabledAtom` gate and its toast behavior.

#### Scenario: Quick capture via leader
- **WHEN** Obsidian is configured and the user presses `Ctrl+Space` then `q`
- **THEN** the quick-capture panel opens

### Requirement: Vault search shortcut
The registry entry `obsidian-vault-search` SHALL be bound to `leader v` (was `Ctrl+Alt+O`; the historical `ctrl+alt+v` in this spec was already stale), keeping the `obsidianEnabledAtom` gate.

#### Scenario: Vault search via leader
- **WHEN** Obsidian is configured and the user presses `Ctrl+Space` then `v`
- **THEN** the vault-search modal opens

### Requirement: Scratchpad toggle shortcut
The system SHALL bind `leader s` to toggle the scratchpad floating panel (was `Ctrl+Alt+L` — the `Ctrl+Alt` tier is removed). `Ctrl+L` remains governed by the app-scope rules (browser URL bar focus outside the terminal; the terminal's clear-screen reachable per ownership mode / passthrough).

#### Scenario: Toggle scratchpad via leader
- **WHEN** the user presses `Ctrl+Space` then `s`
- **THEN** the scratchpad floating panel opens or closes

### Requirement: Context-scoped tab shortcuts inside scratchpad
When the scratchpad panel is open AND focus is contained within the panel subtree, the system SHALL hijack tab-management shortcuts (`Ctrl+Tab`, `Ctrl+Shift+Tab`, `Ctrl+T`, `Ctrl+W`, `Ctrl+Shift+T`) so they operate on scratchpad tabs, scoped via DOM containment — unchanged from today. Other shortcuts keep their global behavior while the scratchpad is focused; the example set updates to the new map (`Ctrl+B`, `Ctrl+1..9`, `leader s`).

#### Scenario: Other shortcuts remain global while scratchpad is focused
- **WHEN** the scratchpad is focused and the user presses `Ctrl+B`, `Ctrl+1..9`, or `Ctrl+Space` then `s`
- **THEN** those fire with their global behavior (sidebar toggle, session switch, scratchpad toggle)

### Requirement: Shortcut registry
All shortcuts SHALL be defined in the central registry (`stores/shortcuts.ts`) as action objects containing: id, label, keys (combo, chord `"leader <key>"`, or `""` for palette-only), category, `scope` (`"global"` default | `"app"`), optional `group` (which-key family), keywords, and handler. The command palette and the dispatcher SHALL consume the same override-resolved registry.

#### Scenario: Registry drives dispatcher, palette and which-key
- **WHEN** the app initializes
- **THEN** the dispatcher, the command palette, and the which-key popover all render from the same resolved registry (no hardcoded duplicate lists)

## REMOVED Requirements

### Requirement: Open current file in Obsidian shortcut
**Reason**: already removed in the implemented reality — "Open in Obsidian" is a surface-scoped bare-letter verb (`o`) handled component-locally in `ObsidianNoteView` (see registry comment in `src/stores/shortcuts.ts`); `ctrl+shift+v` was deliberately freed so paste reaches the terminal. The spec text was stale.

### Requirement: No collision with existing shortcuts
**Reason**: one-time pre-implementation check scoped to the 2026-04 Obsidian change; superseded by the layer conventions, the never-bind list in `docs/shortcuts.md`, and `validateCombo` collision checking at remap time.
