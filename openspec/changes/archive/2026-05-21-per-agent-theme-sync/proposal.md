## Why

When the user flips themes in cluihud (`Settings → Appearance`), the terminal background follows seamlessly for Claude Code sessions but stays painted with the agent's hardcoded TUI palette for pi, opencode, and codex. Root cause is in `src/components/terminal/terminalService.ts:1025-1072`: `paintRow` clears each row with `TERM_THEME.background` and only repaints a per-cell background when `cell.bg !== TERM_THEME.background`. CC mostly leaves `cell.bg = null` so the theme bg shows through; pi/opencode/codex emit explicit ANSI bg color codes for their TUI canvas, locking in their dark palette regardless of cluihud's active theme.

The current agent adapters (`src-tauri/src/agents/{pi,opencode,codex}/adapter.rs`) only inject `CLUIHUD_SESSION_ID` at spawn — no theme hint, no live channel for theme updates. Each TUI ships its own theming subsystem; we should leverage those instead of trying to override their colors at our renderer (fragile heuristic) or sending unsupported OSC 11 sequences (most modern Ink/Bubbletea/Ratatui TUIs don't listen).

This change adds a "per-agent theme sync" capability that maps cluihud's active theme to each agent's native theme format and applies it through the most native channel each agent provides (hot-reloaded JSON for pi, custom-theme file + best-effort live API for opencode, config.toml + next-spawn for codex).

## What Changes

- New `AgentCapability::THEME_SYNC` flag and `AgentAdapter::apply_theme(&self, palette: &ThemePalette)` async method on the adapter trait. Default impl returns `Err(NotSupported(THEME_SYNC))`.
- New shared `ThemePalette` struct in `src-tauri/src/agents/mod.rs` derived from the active cluihud theme (surface, foreground, accent, secondary, muted, border, plus theme id + light/dark variant indicator).
- New Tauri command `apply_theme_to_agents(palette)` invoked from `src/App.tsx` whenever `applyTheme` runs. Iterates registered adapters that advertise `THEME_SYNC` and best-effort applies; failures logged, never surface as user errors.
- **pi adapter**: write `~/.pi/agent/themes/cluihud-active.json` (51-token palette derived from cluihud theme) on every theme change; ensure `~/.pi/agent/settings.json` `"theme": "cluihud-active"` is set once. pi's documented hot-reload picks the file change up live without restart.
- **opencode adapter**: write `~/.config/opencode/themes/cluihud-active.json` (opencode theme schema with `defs` + token map) on every theme change; ensure `~/.config/opencode/tui.json` `"theme": "cluihud-active"` is set once. For live update in active sessions, POST `/tui/execute-command {"command": "theme cluihud-active"}` to the opencode HTTP API on each session's port (best-effort; falls back to apply-on-next-spawn if endpoint rejects the syntax).
- **codex adapter**: write `tui.theme` into `~/.codex/config.toml`. Documented as syntax-highlighting only — does NOT change the TUI canvas background. Surface this limitation in the change log; ship it because (a) it's the only theme key codex exposes today and (b) opens the door for codex CLI to widen its theme surface later.
- CC adapter remains unchanged — does not advertise `THEME_SYNC` (its terminal-default-bg behavior already produces the desired result through the existing renderer).
- No respawn of active sessions on theme change. Each adapter writes through file/HTTP channels; sessions that can hot-reload do so, others apply at next spawn.

## Capabilities

### New Capabilities

- `agent-theme-sync`: Bidirectional theme channel between cluihud's active theme and each non-CC agent. Covers the `ThemePalette` shape, the `THEME_SYNC` capability flag, the `apply_theme` adapter method contract, the Tauri command pipeline from frontend `applyTheme` to backend adapter dispatch, and the per-agent fallback policy (hot-reload preferred → live API → next-spawn).

### Modified Capabilities

- `agent-adapter` — adds `THEME_SYNC` to the `AgentCapability` flag set and adds `apply_theme` to the trait surface.
- `pi-adapter` — adds the requirement that pi advertises `THEME_SYNC` and implements it via `~/.pi/agent/themes/cluihud-active.json` hot-reload.
- `opencode-adapter` — adds the requirement that opencode advertises `THEME_SYNC` and implements it via custom theme file + best-effort live `/tui/execute-command`.
- `codex-adapter` — adds the requirement that codex advertises `THEME_SYNC` with the documented limitation (syntax-highlight only, next-spawn).

## Impact

- **Backend**: `src-tauri/src/agents/mod.rs` (trait + capability flag + `ThemePalette`), `src-tauri/src/agents/{pi,opencode,codex}/adapter.rs` (apply_theme impls), `src-tauri/src/agents/registry.rs` (dispatch helper), `src-tauri/src/commands.rs` (Tauri command + IPC types), `src-tauri/src/lib.rs` (command registration).
- **Frontend**: `src/App.tsx` (call `apply_theme_to_agents` post-`applyTheme`), `src/lib/themes.ts` (export palette extraction helper that reads computed CSS tokens for the active theme), `src/lib/types.ts` (new `ThemePalette` type mirroring backend).
- **Filesystem**: writes to `~/.pi/agent/themes/`, `~/.pi/agent/settings.json`, `~/.config/opencode/themes/`, `~/.config/opencode/tui.json`, `~/.codex/config.toml`. All writes are idempotent and namespaced under `cluihud-active` to avoid clobbering user-authored themes.
- **No schema changes**, no DB migrations, no new dependencies.

## Build contract

### Qué construyo
- `AgentCapability::THEME_SYNC` flag + `AgentAdapter::apply_theme` async method (default `NotSupported`).
- `ThemePalette` struct in `src-tauri/src/agents/mod.rs` + matching TS type in `src/lib/types.ts`.
- `apply_theme_to_agents` Tauri command that dispatches the palette to every adapter advertising `THEME_SYNC`.
- pi adapter `apply_theme` writing `cluihud-active.json` + ensuring settings.json points at it.
- opencode adapter `apply_theme` writing the custom theme JSON + ensuring tui.json points at it + best-effort live `/tui/execute-command` POST.
- codex adapter `apply_theme` writing `tui.theme` into config.toml.
- Frontend wiring in `App.tsx` that derives the palette from computed CSS tokens and invokes the command after `applyTheme`.
- Spec deltas under `openspec/changes/per-agent-theme-sync/specs/`.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual walk: launch `pnpm tauri dev`, open one session per agent (CC, pi, opencode, codex if installed), flip cluihud theme three times across diverse themes (v1-dark, v6-nothing, v11-tokyo-night) — verify pi background follows live, opencode background follows on next prompt cycle (or live if `/tui/execute-command` succeeded), codex `~/.codex/config.toml` updated (background unchanged), CC unaffected.
- Unit tests per adapter: `apply_theme_writes_expected_files`, `apply_theme_is_idempotent`, `apply_theme_does_not_clobber_user_themes`.

### Criterio de done
- Switching cluihud theme produces a live background change in pi sessions within ≤1s (no restart needed).
- For opencode: either live change via API, or correct theme file written so next session opens with the new palette.
- For codex: `~/.codex/config.toml` reflects the new `tui.theme`, with an acknowledged limitation that the canvas background does NOT change live.
- CC sessions remain identical to today (no regression).
- All verification commands pass.
- The change documents in implementation.md that opencode/codex have known fidelity gaps and proposes phase-2 follow-ups in the change description.

### Estimated scope
- files_estimate: 10
- risk_tier: medium
- tags: [feature, agent-agnostic]
- visibility: private
- spec_target: agent-adapter, pi-adapter, opencode-adapter, codex-adapter
