## Why

Front/back split projects need shells running in different directories: working the frontend often means starting the backend from a sibling dir. Today every quake shell spawns in the session's cwd, so a remembered command like `pnpm dev` for the backend re-runs in the wrong directory after a re-open. Follow-up to the archived `quake-terminal` change, requested from daily use.

## What Changes

- **Per-shell working directory** on `EnvShellDef` (`cwd`, optional; relative paths resolve against the session cwd). No migration — the JSON column absorbs the field.
- **Capture on submit**: at Enter the shell process's actual cwd is read from `/proc/<pid>/cwd` (Linux, `cfg`-gated) and persisted with the tab when it differs from the session cwd.
- **Respawn in the stored dir**: re-open spawns the shell directly in its remembered cwd (exists-check, fallback to the session cwd) — the pre-filled command stays pure, no `cd &&` prefix.
- **Modal + suggestions**: the environment-shells rows (new-session modal) and the per-workspace suggestion library gain an optional cwd field; quick-pick carries it.

## Capabilities

### Modified Capabilities

- `quake-terminal`: the Environment shells requirement gains per-shell cwd (definition, capture, respawn).

## Impact

- Backend: `pty.rs` (child pid retention, `/proc` cwd read, payload field, spawn dir resolution), `models.rs` (`EnvShellDef.cwd`).
- Frontend: `quake.ts` (capture/persist/spawn), `terminalService.ts` (`shellCwd` pass-through), `QuakeTerminal.tsx`, `AgentPickerModal.tsx` + `SettingsPanel.tsx` (cwd inputs).

## Build contract

### Qué construyo

- `EnvShellDef.cwd` end-to-end: capture at submit, persist, respawn in dir, modal + suggestions inputs.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual: run a command from a `cd ../other-dir` shell, re-open the session → tab respawns inside `other-dir` with the command pre-filled; modal/suggestion row with relative cwd spawns there.

### Criterio de done

- Re-opened shells land in their remembered directory; missing dirs fall back to the session cwd; machine checks green.

### Estimated scope

- files_estimate: 7
- risk_tier: low
- tags: [feature]
- visibility: public
- spec_target: quake-terminal
