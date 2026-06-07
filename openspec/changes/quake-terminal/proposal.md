## Why

The user runs project environments (`pnpm dev`, `docker compose up`, often 3+ at once) in terminals kept open *outside* Nergal, defeating the point of a single keyboard-first workspace. Today every PTY in cluihud is bound to an agent session — there is no plain shell. A recent footgun made this concrete: the launch-options `startup_command` runs `cd <cwd> && <cmd> && claude`, so a long-running command (`pnpm start`) blocks and the agent never launches. We need first-class auxiliary shells, and a clean split between "quick prelude that exits" and "long-running environment command".

The backend cost is low: a session's terminal is already a shell PTY (`spawn_pty`) into which `start_claude_session` *writes the agent command*. An auxiliary shell is the same machinery without that write. The real work is UI/model: a surface to hold N shells, a multi-host terminal renderer, and per-session persistence.

## What Changes

- **Quake terminal**: a full-width overlay (toggled with `Ctrl+}`) that drops below the TopBar over the content area, holding N shell tabs for the active session. Resizable height; stays visible when focus leaves so logs are readable while typing elsewhere.
- **New focus zone `quake`** with its own accent border that follows focus. `Ctrl+}` semantics: hidden→open+focus; visible+unfocused→focus; visible+focused→hide. Not in the `alt+left/right` cycle.
- **Per-session auxiliary shells**: shell PTYs owned by a session (die with it), keyed under the session id. Rendered by the existing canvas renderer, generalized from a single global host to **per-region hosts** (center = agent, quake = shells).
- **Two command concepts** (footgun fix): the existing launch-options `startup_command` is reframed as **prelude** (quick, must exit, runs in the agent terminal so the agent inherits the env); a new **environment shells** list `(label, command)` spawns quake shells for long-running commands.
- **Environment shells in the new-session modal** + per-session persistence (DB). Run behavior: auto-run on session creation, pre-fill (Enter to run) on re-open after restart.
- **Per-workspace environment-shell suggestions** (settings, mirrors the openspec-dir override / obsidian config pattern): a library of `(label, command)` presets per workspace, quick-picked when creating a session.

## Capabilities

### New Capabilities

- `quake-terminal`: Per-session auxiliary shell surface. Covers the Quake overlay, the `quake` focus zone + `Ctrl+}`, N shell tabs, per-region terminal rendering, per-session shell lifecycle, environment shells (definition + run behavior + persistence), and per-workspace suggestions.

### Modified Capabilities

- `terminal-wezterm`: the renderer's single-host model becomes per-region (a second host for the quake). Additive — the agent terminal path is unchanged.
- `session-launch-options`: `startup_command` is reframed as a prelude that must exit; long-running commands move to environment shells. (No behavior change to existing preludes; UI copy + a hint.)

## Impact

- **Backend**: `pty.rs` (shell-only PTY spawn path, no agent command), new shell-id model keyed under sessions, new Tauri commands (spawn/kill/list aux shells), DB migration for per-session environment-shell defs + per-workspace suggestions, `models.rs`.
- **Frontend**: `terminalService.ts` per-region host refactor, new Quake overlay components + store, `shortcuts.ts` (`Ctrl+}` + `quake` zone), `Workspace.tsx` (mount the overlay + accent border), `AgentPickerModal.tsx` (environment shells section + suggestion quick-pick), `SettingsPanel.tsx` (per-workspace suggestions library).
- **File system / DB**: new columns/tables for env-shell defs (per session) and suggestions (per workspace).
- **Existing flows**: launch-options prelude semantics clarified; the `ports` status-bar chip surfaces dev servers from quake shells for free (existing `/proc/net/tcp` scanner).

## Build contract

### Qué construyo

- Quake terminal overlay (`Ctrl+}`, resizable, N shell tabs, new-shell button) as a new `quake` focus zone with accent border.
- Per-region terminal renderer (`terminalService` multi-host) so agent + shells render simultaneously.
- Backend shell-only PTY spawn + per-session shell lifecycle + Tauri commands.
- Environment shells: modal section, per-session DB persistence, auto-run-first / pre-fill-on-reopen.
- Prelude reframe of the launch-options `startup_command`.
- Per-workspace environment-shell suggestions library in settings + quick-pick in the modal.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual: `Ctrl+}` toggles/focuses the overlay; open 3 shells, run `pnpm dev`, see it in the `ports` chip; switch sessions and the quake content swaps; close a session and its shells die; create a session with env shells (auto-run) then reopen after restart (pre-filled); pick a per-workspace suggestion in the modal.

### Criterio de done

- Agent terminal and quake shells render simultaneously without the single-host conflict.
- Shells are per-session and die with the session; defs persist and respawn per the run-behavior rule.
- Prelude no longer blocks the agent; long-running commands live in quake shells.
- All machine checks green.

### Estimated scope

- files_estimate: 14
- risk_tier: medium
- tags: [feature, migration]
- visibility: public
- spec_target: quake-terminal
