# Design — Quake terminal

Design conversation held 2026-06-06 (user + Claude). All decisions are final; this records the *why* behind them. Memory anchor: `project_quake_terminal.md`.

## Decision log

### Surface: Quake overlay (not sidebar entry, not right-panel, not split)

Three placements were weighed:
- **Sidebar entry** (non-agent session): near-zero cost (reuses everything), but shell and agent share the center pane one-at-a-time — doesn't solve "watch the dev server while working with the agent".
- **Right-panel view**: reuses the panel tab system, but competes with plan/git/specs for the single right-panel slot and is narrow. Constant view-switching for someone running envs daily.
- **Quake overlay** (chosen): full-width drop-down on an independent axis. No permanent layout cost, full width for logs, doesn't compete with review panels. Matches the "drop → levanto ambiente → despacho" workflow.

### Focus + shortcut

- `Ctrl+}` (verified no collision in `shortcuts.ts`; wire respecting `event.code` / WebKitGTK — `}` is `BracketRight`+Shift). 
- New focus zone `quake` (current: `sidebar | terminal | panel` in `shortcuts.ts:37`). It gets a `data-focus-zone='quake'` container and the accent border that the focus system already drives.
- The overlay **stays visible when focus leaves** (read logs while typing in the agent terminal / right panel). The border follows focus.
- `Ctrl+}` cycle: hidden→open+focus; visible+unfocused→focus; visible+focused→hide.
- Deliberately **not** in the `alt+left/right` cycle (`getVisibleZones`) — it's an overlay, reached only via `Ctrl+}`. (Revisit if the user wants it in the cycle — small change.)

### Scope: per-session

Shells belong to the active session (its worktree cwd); switching sessions swaps the quake content; closing a session kills its shells. Rationale: the user chose per-session over per-workspace. Suggestions are per-*workspace* (below) because stacks differ by project, but the live shells are per-session.

### Renderer: single-host → per-region hosts

`terminalService` (canvas, outside React) currently has one global `host` and shows one terminal at a time (`show()` moves the active session's container into the host, hides others). The quake needs a **second host** so the agent terminal (center) and a shell (quake) render simultaneously. Generalize to per-region hosts: each region (`center`, `quake`) has its own host element and its own "active terminal". The renderer is already agent-agnostic (it paints a grid from any PTY), so no per-agent special-casing.

### Backend: shell = agentless PTY

`spawn_pty` already creates a shell PTY; `start_claude_session` is what writes `cd <cwd> && claude`. An aux shell is `spawn_pty` **without** that write. Key aux-shell PTYs under the owning session id so teardown is automatic. New Tauri commands: spawn/kill/list aux shells for a session.

### Two command concepts (the footgun fix)

The launch-options `startup_command` runs `cd <cwd> && <cmd> && claude`; a non-exiting command (`pnpm start`) blocks `&& claude`. Split:
- **Prelude** (reframe of `startup_command`): quick, MUST exit (`nvm use`, `source .env`); runs in the agent terminal so the agent inherits the env. Keep behavior; add a hint.
- **Environment shells** (new): `(label, command)` list → each spawns a quake shell. Home for long-running commands. They never touch the agent terminal.

### Environment-shell run behavior

Auto-run on session **creation** (levantás el ambiente sin fricción); **pre-fill** (command typed, Enter to run) on **re-open** after restart (avoids relaunching heavy processes unasked; the process died on close anyway). Chosen over pure auto-run and pure pre-fill.

### Persistence

Persist environment-shell defs `(label, command)` per session in the DB (migration). Re-open recreates the tabs with the command pre-filled. Ad-hoc `+` shells (no command) are ephemeral. PTYs can't survive process exit, so "remember the set" = remember the defs, not the live processes/scrollback.

### Per-workspace suggestions

A settings library of `(label, command)` env-shell presets **per workspace** (stacks differ between projects; a global library would be noise). Mirrors the per-workspace config pattern already used for the openspec-dir override and obsidian config (DB row keyed by workspace_id). At session creation, quick-pick from the active workspace's suggestions to populate the environment-shells list.

## Open questions for implementation

- Exact `Ctrl+}` binding representation in `shortcuts.ts` (the registry mixes key-char like `ctrl+ñ` and code-based handling — confirm the matcher path for `BracketRight`+Shift). → Resolved: dual `e.key === "}"` / `ctrl+shift+BracketRight` matching in the capture-phase dispatcher (the glyph is layout-dependent).
- Per-region host API shape in `terminalService` (region enum vs host registry). → Resolved: `Region = "center" | "quake"` union + per-region host/active maps.
- Whether ad-hoc shells should optionally be promotable to persisted env shells. → Resolved by user testing feedback (2026-06-07): the persisted `env_shells` column holds the session's **live tab set** — ad-hoc tabs join it and a per-shell input-line tracker (backend, alt-screen-filtered) remembers the last submitted command, so re-open pre-fills everything that ran. Modal defs are just the seed.
- Registro de context-bridge channels could later render as a read-only quake shell (cross-feature synergy) — out of scope here, noted in `Context-bridge re-análisis` (vault).

## Post-testing refinements (2026-06-07)

- **Per-session visibility**: quake open/closed is a per-session map (like the right panel's collapsed state), not global.
- **Self-exit detection**: `exit` in a shell retires its tab. Root cause for it not working: the slave PTY fd was retained in `PtyInstance`, so the master never reached EOF — the slave is now dropped post-spawn. Kill-driven EOFs are filtered (de-register before drop) so teardown can't masquerade as self-exit and erase the persisted set.
- **Quake-scoped shortcuts**: `Ctrl+W` closes the active tab (capture-phase override; globally it soft-closes the session), `Ctrl+Shift+T` opens a new one (plain `Ctrl+T` belongs to the shell). Trade-off: `Ctrl+W` no longer reaches the shell as werase while the quake is focused.

## Non-goals

- No cross-session communication (that's context-bridge, a separate follow-up).
- No multiplatform work (Linux-only assumptions stay).
- No change to the agent terminal's behavior beyond the renderer host generalization.
