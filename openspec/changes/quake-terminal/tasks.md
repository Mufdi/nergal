# Tasks — Quake terminal

Phased per the design. Each phase is independently verifiable (`cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`). Implementation happens in a fresh session.

## 1. Foundation — per-region renderer + agentless shell PTY

- [ ] 1.1 Backend: add a shell-only spawn path. Factor `spawn_pty` so a PTY can be created for an aux shell **without** the `start_claude_session` agent-command write. Key aux shells under `{session_id}:{shell_id}` in `PtyManager`.
- [ ] 1.2 Backend: Tauri commands `spawn_aux_shell(session_id, shell_id, cwd, command?)`, `kill_aux_shell(session_id, shell_id)`, `list_aux_shells(session_id)`. On session teardown (`delete_session`), kill all aux shells for that session.
- [ ] 1.3 Frontend: generalize `terminalService` from a single global `host` to **per-region hosts** (`center`, `quake`). `setHost(region, el)`, and `show/hide/focus/fit/destroy` take a region (default `center` for back-compat). Each region tracks its own active terminal.
- [ ] 1.4 Verify: agent terminal still works unchanged through the `center` region; a manually-spawned shell renders in a second host.

## 2. Quake overlay UI + focus zone

- [ ] 2.1 `shortcuts.ts`: add `FocusZone` value `quake`; add the `Ctrl+}` binding (respect `event.code` = `BracketRight`+Shift) with the hidden→open+focus / visible+unfocused→focus / visible+focused→hide semantics. Keep `quake` out of `getVisibleZones` (alt+left/right cycle).
- [ ] 2.2 New store `stores/quake.ts`: `quakeOpenAtom`, per-session shell list atom, active-shell-per-session atom.
- [ ] 2.3 New components: `QuakeTerminal` overlay (resizable height, `data-focus-zone='quake'`, accent border via the focus system) + a tab strip (`QuakeTabStrip`) with shell tabs + new-shell button + close.
- [ ] 2.4 Mount `QuakeTerminal` in `Workspace.tsx`; wire the host to the `quake` region; swap shells on active-session change; ad-hoc `+` spawns an empty shell in the session cwd.
- [ ] 2.5 Verify: `Ctrl+}` cycle; open 3 ad-hoc shells; run `pnpm dev`, confirm it appears in the `ports` status-bar chip; switch sessions swaps content; close session kills shells.

## 3. Environment shells — modal + persistence

- [ ] 3.1 Migration: per-session environment-shell defs `(label, command, order)` (JSON column on `sessions` or a child table). `models.rs` + db.rs accessors + round-trip test.
- [ ] 3.2 `AgentPickerModal`: add an "Environment shells" section (add/remove rows of label+command), keyboard-navigable, consistent with the launch-options list.
- [ ] 3.3 `create_session`: persist env-shell defs; on session start spawn the quake shells with **auto-run**.
- [ ] 3.4 Resume path: on re-open, respawn env shells with the command **pre-filled** (typed, not executed). Distinguish first-creation vs re-open.
- [ ] 3.5 Verify: create with env shells (auto-run); restart Nergal; reopen session (pre-filled, Enter re-runs); defs survive.

## 4. Per-workspace suggestions

- [ ] 4.1 Migration: per-workspace env-shell suggestions `(label, command)` (extend `workspace_config` or a child table). db.rs accessors + Tauri get/set commands (mirror openspec-dir override).
- [ ] 4.2 Settings → a section to manage the active workspace's suggestion library (add/edit/remove), per-workspace selector like the OpenSpec/Obsidian sections.
- [ ] 4.3 `AgentPickerModal`: quick-pick chips/list from the active workspace's suggestions to populate the environment-shells section.
- [ ] 4.4 Verify: suggestions scoped per workspace; quick-pick populates the modal.

## 5. Polish + close

- [ ] 5.1 Reframe the launch-options `startup_command` UI as "Prelude" with a hint that it must exit (point long-running commands to environment shells). Update `docs/patterns.md` / `design.md` if a new pattern emerges.
- [ ] 5.2 Update specs: this change's delta + reconcile `terminal-wezterm` (per-region) and `session-launch-options` (prelude) on archive.
- [ ] 5.3 Full check green: `cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`.
- [ ] 5.4 Manual UX walk of every scenario in the spec before `/openspec-sync` archive.
