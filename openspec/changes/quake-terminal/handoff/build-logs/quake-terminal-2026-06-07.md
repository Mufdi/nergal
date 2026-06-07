# Session quake-terminal · 2026-06-07

## Tool activity

- Read: ~20 (pty.rs, terminalService.ts, shortcuts.ts, useKeyboardShortcuts.ts, Workspace.tsx, TerminalManager.tsx, Sidebar.tsx, AgentPickerModal.tsx, SettingsPanel.tsx, db.rs, models.rs, commands.rs, lib.rs, docs)
- Edit/Write: ~40
- Bash (verify): cargo check ×2, cargo clippy ×3, cargo test ×4, cargo fmt ×2, tsc ×5

## File changes

Backend:
- `src-tauri/src/pty.rs`: `aux_shells` map, `agent_session` flag on `spawn_pty` (env + EOF divergence), `spawn_aux_shell`/`kill_aux_shell`/`list_aux_shells`, `kill_session_pty` kills aux shells, `shell:exited` event
- `src-tauri/src/models.rs`: `EnvShellDef`, `Session.env_shells`
- `src-tauri/src/db.rs`: `parse_env_shells`, env_shells in SELECT/INSERT, workspace suggestions get/set, 2 round-trip tests
- `src-tauri/src/commands.rs`: `create_session` env_shells param, suggestions get/set commands
- `src-tauri/src/lib.rs`: 5 new invoke handlers
- `src-tauri/migrations/013_env_shells.sql`, `014_env_shell_suggestions.sql`

Frontend:
- `src/components/terminal/terminalService.ts`: per-region hosts (`Region = center|quake`), `showTerminal` factored from `show`, `showShell`, `dropShellEntry`, region-aware `setHost/hideAll/fitActive/focusActive`, `destroy` retires aux entries, `focus` param
- `src/stores/quake.ts` (new): atoms, `addAdHocShell`, `removeShell`, `spawnEnvShells`, `shell:exited` listener
- `src/components/quake/QuakeTerminal.tsx` (new): overlay + tab strip + resize handle + empty state
- `src/stores/shortcuts.ts`: `quake` zone, `toggleQuake`, registry entry
- `src/hooks/useKeyboardShortcuts.ts`: Ctrl+} dual key/code matcher
- `src/components/layout/Workspace.tsx`: mount overlay
- `src/components/terminal/TerminalManager.tsx`: re-open prefill seeding
- `src/components/layout/Sidebar.tsx`: creation flow auto-run + workspaceId prop
- `src/components/session/AgentPickerModal.tsx`: env shells section + suggestion chips + prelude reframe
- `src/components/settings/SettingsPanel.tsx`: `EnvShellSuggestionsField`
- `src/stores/workspace.ts`: `EnvShellDef`, `Session.env_shells`
- `docs/patterns.md`: quake zone in §5.1

## Decisiones + tradeoffs

- Aux shells register in `session_ptys` under `{session_id}::{shell_id}` — every existing terminal_* command works for shells with zero new IPC surface. Rationale: smallest possible backend delta.
- Aux shells DON'T get `CLUIHUD_SESSION_ID`: an agent manually launched inside one must not route hook events into the owning session's panels.
- `Ctrl+}` matches `e.key === "}"` OR `ctrl+shift+BracketRight`: the glyph is layout-dependent (Spanish variants type it via AltGr, where altKey reporting is unreliable); code-only matching per the brief would target `*` on the user's layout.
- Pre-fill = write command bytes WITHOUT trailing newline into the fresh PTY prompt — no TUI state needed, Enter executes.
- Auto-run spawns headless at a 200x50 placeholder grid; `showTerminal` aligns the PTY via `resize_session_terminal` on first view (quake region only — center stays byte-identical).
- `showShell` never steals focus (`focus: false` + explicit assertion when the quake zone owns focus) — session switches with the quake open keep the user's zone.
- Suggestions field persists on mutation/blur instead of the Settings Save: list edits are discrete actions, unlike path typing (which got the draft+Save treatment).
- Orchestrator-as-builder instead of Task-spawned builders: the design context (5-phase plan, watch-outs, conventions) was already loaded in-session; fresh-context builders would re-read everything per phase. Reviewer pass still independent.

## Divergencias vs proposal

- tasks.md 1.2 said teardown on `delete_session`; landed on `kill_session_pty` (which every delete path — pendingDeletes, sessionTabs — already invokes via `terminalService.destroy`). Same guarantee, one path.
- tasks.md 2.3 named a separate `QuakeTabStrip` component; the strip is inline in `QuakeTerminal` (no reuse case yet — YAGNI).
- tasks.md 3.4 "respawn on re-open" implemented as lazy spawn-on-view (tabs seed eagerly, PTY spawns when the shell is first shown) — pre-fill semantics make eager respawn pure waste.
- Spec scenario "auto-spawn first ad-hoc shell on first open" wasn't in the spec; added as a reversible UX default (open quake with no shells → one shell, vs. empty state).

## Round 2 — user testing feedback (2026-06-07)

1. `exit` didn't close the tab → root cause: slave PTY fd retained in `PtyInstance` kept the master from EOF. Slave dropped post-spawn; `PtyInstance.pair` → `master`. Kill-driven EOFs filtered via "still registered in session_ptys" guard so teardown can't erase the persisted set.
2. Quake visibility now per-session (`quakeOpenMapAtom`), mirroring the right panel's collapsed map.
3. Quake-focused `Ctrl+W` closes the active tab (was soft-closing the session), `Ctrl+Shift+T` opens a new one — capture-phase overrides in useKeyboardShortcuts.
4. Modal env-shells section keyboard-reachable: optIdx space extended (startup → env rows → add-row), focus effect drives the command inputs, add-row activates with Space/Enter.
5. Spaces in env inputs were swallowed by the container's Space-toggle (inputs didn't track optIdx) → onFocus now sets the row index.
6. Re-open lost ad-hoc work → `env_shells` reframed as the session's live tab set: `update_session_env_shells` persists on add/remove/command-submit; backend per-shell input-line tracker (`aux_input_lines`, alt-screen-filtered, prefill-seeded, paste-aware) emits `shell:command` on Enter.

Full check re-run green (clippy -D warnings, 322 tests, fmt, tsc); change re-validated.

## Round 3 — user testing feedback (2026-06-07)

1. Ctrl+Tab / Ctrl+Shift+Tab cycle quake tabs when the zone holds focus (`cycleQuakeShell` branch in nextTab/prevTab).
2. Modal fully arrow-navigable: suggestion chips are a row in the optIdx space (←→ between chips, Enter/Space adds); env rows hop label↔command↔✕ at caret boundaries; ✕ focusable + Enter/Space deletes.
3. Resume prefill missing on shell 2 → root cause: keystroke mirroring can't see history recall (↑), tab completion, or autosuggest accepts. Rewrote capture: `AuxLineTracker` anchors the line-start column at the first key of the line (cursor pos pre-echo, prefill chars discounted); at Enter the command is read off the GRID (`cursor_line_text` in session.rs). Paste anchors too; wrapped multi-row commands truncate (accepted limitation).
4. Browser panel painted over the quake → BrowserHost is an iframe portaled to body at z-40 (same z, later DOM order wins). Quake raised to z-[45] (below dialogs/zen at 50+).

Full check green; change re-validated.
