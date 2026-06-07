# Tasks — Quake terminal

Phased per the design. Each phase is independently verifiable (`cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`). Implementation happens in a fresh session.

## 1. Foundation — per-region renderer + agentless shell PTY

- [x] 1.1 Backend: add a shell-only spawn path. Factor `spawn_pty` so a PTY can be created for an aux shell **without** the `start_claude_session` agent-command write. Key aux shells under `{session_id}::{shell_id}` in `PtyManager`.
- [x] 1.2 Backend: Tauri commands `spawn_aux_shell(session_id, shell_id, cwd, command?, autorun)`, `kill_aux_shell(session_id, shell_id)`, `list_aux_shells(session_id)`. On session teardown (`kill_session_pty`, invoked by every delete path), kill all aux shells for that session.
- [x] 1.3 Frontend: generalize `terminalService` from a single global `host` to **per-region hosts** (`center`, `quake`). `setHost(el, region)`, and `show/hide/focus/fit` take a region (default `center` for back-compat). Each region tracks its own active terminal.
- [ ] 1.4 Verify: agent terminal still works unchanged through the `center` region; a manually-spawned shell renders in a second host. *(machine checks green; manual walk pending)*

## 2. Quake overlay UI + focus zone

- [x] 2.1 `shortcuts.ts`: add `FocusZone` value `quake`; add the `Ctrl+}` binding (dual `event.key`/`event.code` matching in `useKeyboardShortcuts` — the `}` glyph is layout-dependent) with the hidden→open+focus / visible+unfocused→focus / visible+focused→hide semantics. Keep `quake` out of `getVisibleZones` (alt+left/right cycle).
- [x] 2.2 New store `stores/quake.ts`: `quakeOpenAtom`, per-session shell list atom, active-shell-per-session atom.
- [x] 2.3 New component: `QuakeTerminal` overlay (resizable height, `data-focus-zone='quake'`, accent border via the focus system) with an inline tab strip: shell tabs + new-shell button + close.
- [x] 2.4 Mount `QuakeTerminal` in `Workspace.tsx`; wire the host to the `quake` region; swap shells on active-session change; ad-hoc `+` spawns an empty shell in the session cwd.
- [ ] 2.5 Verify: `Ctrl+}` cycle; open 3 ad-hoc shells; run `pnpm dev`, confirm it appears in the `ports` status-bar chip; switch sessions swaps content; close session kills shells. *(manual walk pending)*

## 3. Environment shells — modal + persistence

- [x] 3.1 Migration (013): per-session environment-shell defs as a JSON column on `sessions`. `models.rs` `EnvShellDef` + db.rs accessors + round-trip test.
- [x] 3.2 `AgentPickerModal`: add an "Environment shells" section (add/remove rows of label+command, Enter commits), consistent with the launch-options list.
- [x] 3.3 `create_session`: persist env-shell defs; the creation flow spawns the quake shells with **auto-run** (headless — quake need not be open).
- [x] 3.4 Resume path: on re-open, seed quake tabs from the persisted defs; shells spawn lazily on view with the command **pre-filled** (typed, not executed). First-creation vs re-open distinguished by the creation flow seeding the tab atom before activation.
- [ ] 3.5 Verify: create with env shells (auto-run); restart Nergal; reopen session (pre-filled, Enter re-runs); defs survive. *(manual walk pending)*

## 4. Per-workspace suggestions

- [x] 4.1 Migration (014): per-workspace env-shell suggestions as a JSON column on `workspace_config`. db.rs accessors + Tauri get/set commands (mirror openspec-dir override) + round-trip test.
- [x] 4.2 Settings → Paths: `EnvShellSuggestionsField` manages the workspace's suggestion library (add/edit/remove, persists on mutation/blur), per-workspace selector like the OpenSpec section.
- [x] 4.3 `AgentPickerModal`: quick-pick chips from the workspace's suggestions populate the environment-shells section (added entries dim their chip).
- [ ] 4.4 Verify: suggestions scoped per workspace; quick-pick populates the modal. *(manual walk pending)*

## 5. Polish + close

- [x] 5.1 Reframe the launch-options `startup_command` UI as "Prelude" with a hint that it must exit (point long-running commands to environment shells). `docs/patterns.md` §5.1 documents the quake zone's cycle-exclusion.
- [ ] 5.2 Update specs: reconcile `terminal-wezterm` (per-region) and `session-launch-options` (prelude) on archive via `/openspec-sync`.
- [x] 5.3 Full check green: `cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit` (322 tests).
- [ ] 5.4 Manual UX walk of every scenario in the spec before `/openspec-sync` archive.
