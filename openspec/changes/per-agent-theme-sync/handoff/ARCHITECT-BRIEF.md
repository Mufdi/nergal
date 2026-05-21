# Architect Brief — Per-Agent Theme Sync

## Project mission

> Nergal corre **alrededor** del agente CLI, no en su lugar. Augmenta y organiza el ecosistema; no reescribe lo que el agente ya hace.

This change embodies the mission: instead of reaching into each TUI's rendering pipeline (which would mean fighting their internal palette), we leverage each agent's documented theming subsystem and let them paint themselves with our colors.

## Sprint Contract

Lives in `proposal.md` under `## Build contract`. Summary:

- **Build**: `THEME_SYNC` capability + `apply_theme` trait method + ThemePalette IPC type + per-adapter impls (pi live, opencode partial, codex limited) + frontend wiring in `App.tsx`.
- **Verify**: clippy + cargo test + cargo fmt + tsc + manual walk across agents.
- **Done**: pi background follows live; opencode follows on next spawn (or live if API supports it); codex config updated with documented bg-unchanged limitation; CC untouched.
- **Estimate**: ~10 files, medium risk, tags `[feature, agent-agnostic]`, deltas to 4 specs.

## Context primer

- Root cause (with file refs): `src/components/terminal/terminalService.ts:1025-1072` `paintRow`. CC's TUI leaves `cell.bg = null` → falls back to `TERM_THEME.background`. pi/opencode/codex emit explicit ANSI bg → cell painted with hardcoded color regardless of theme.
- Existing capability surface: see `openspec/specs/agent-adapter/spec.md` — bitflag set is explicit, designed for additive growth. `THEME_SYNC` is a clean fit.
- Existing adapter spawn pattern: `src-tauri/src/agents/*/adapter.rs` Self::new declares capabilities, spawn returns SpawnSpec with `CLUIHUD_SESSION_ID` env. Adding `apply_theme` follows the same shape (sync constructor declares flag, async method implements).
- Frontend theme apply flow: `src/App.tsx:55-63` `useEffect` calls `applyTheme(themeMode, customThemes)` from `src/lib/themes.ts`. This is the single hook point for downstream sync.

## Per-agent fidelity matrix

| Agent | Channel | Live? | Restart? | Background follows? | Notes |
|---|---|---|---|---|---|
| Claude Code | n/a (renderer fallback already works) | YES | No | YES | Untouched. |
| pi | `~/.pi/agent/themes/cluihud-active.json` hot-reload | YES | No | YES | pi's docs explicitly document hot-reload of the active custom theme file. |
| opencode | `~/.config/opencode/themes/cluihud-active.json` + `tui.json` + best-effort `/tui/execute-command` | PARTIAL | Maybe | YES on next spawn; live if API accepts | Spike during implementation to confirm `/tui/execute-command "theme NAME"` works. |
| codex | `~/.codex/config.toml` `tui.theme` | NO | YES | NO | Syntax-highlight only by codex's documented behavior. Documented limitation. |

## Dependencies / blockers

- `toml_edit` crate (likely already in Cargo.toml; verify at start). If absent, add to `[dependencies]`.
- `dirs` crate for HOME resolution (already present per existing adapter code).
- Existing `reqwest` client setup in `src-tauri/src/agents/opencode/sse_client.rs` — reuse the same client config.
- No new frontend deps; debounce can be inline (~10 lines).

## Spike checkpoints

1. **opencode live switch** (Phase 4.1 in tasks.md): determine if POST `/tui/execute-command {"command":"theme NAME"}` switches theme live. Block on this before writing the HTTP branch. Result → `handoff/spike-opencode-live-switch.md`.
2. **codex syntax theme names** (Phase 5.1 in tasks.md): confirm available theme names (likely just `"monochrome"`). Result → inline in implementation.md mapping table or `handoff/spike-codex-themes.md`.

## Gating decision

- `risk_tier: medium`, `files_estimate: 10` (less than 5 trigger threshold? — yes, ≥5 triggers iprev. So iprev WOULD apply on execute).
- Tags include `feature` — not `migration`/`security`/`breaking-change`. No auto-escalation.
- **iterative-plan-review eligible** (`files_estimate ≥ 5`). The user paused before implementation, so iprev runs at execute time, NOT now.

## Out-of-scope guardrails

- No respawn of active sessions on theme change.
- No new user settings (theme sync is automatic; users opt out by selecting a non-`cluihud-active` theme in the agent's own settings).
- No changes to CC adapter or terminal renderer.
- No OSC 11 path (kept as future enhancement note in design.md).
- No CHANGELOG/version bump in this change — handled by /openspec-sync post-archive.

## Verification on exit

When implementation completes and before archive:

1. `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
2. `npx tsc --noEmit`
3. Manual walk per `proposal.md` Build contract → record in `handoff/manual-verification.md`.
4. `openspec validate per-agent-theme-sync` (or `/openspec-sync` equivalent).
