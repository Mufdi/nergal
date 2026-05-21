# Tasks — Per-Agent Theme Sync

> Read `implementation.md` first — it has the file-by-file plan and order of operations.

## Phase 1 — Trait surface & shared types

- [ ] 1.1 Add `AgentCapability::THEME_SYNC` to the bitflag set in `src-tauri/src/agents/mod.rs`. Update the serialization match arm so the wire format becomes `"THEME_SYNC"`. Update the agent-adapter spec capability list (delta in `specs/agent-adapter/spec.md`).
- [ ] 1.2 Add `ThemePalette` struct (fields per design.md §Decision 3) in `src-tauri/src/agents/mod.rs`. Derive `Clone`, `Debug`, `Serialize`, `Deserialize`. Add a unit test `theme_palette_round_trips_through_serde`.
- [ ] 1.3 Add `async fn apply_theme(&self, _palette: &ThemePalette) -> Result<(), AdapterError>` to the `AgentAdapter` trait with default impl returning `Err(AdapterError::NotSupported { capability: AgentCapability::THEME_SYNC })`. Document the contract in the doc comment.
- [ ] 1.4 Add the matching TypeScript type to `src/lib/types.ts`: `export interface ThemePalette { id: string; isDark: boolean; surface: string; foreground: string; card: string; secondary: string; mutedForeground: string; border: string; accent: string; }`.

## Phase 2 — Registry dispatch & Tauri command

- [ ] 2.1 In `src-tauri/src/agents/registry.rs`, add `async fn apply_theme_to_all(&self, palette: ThemePalette)`: iterate adapters with `capabilities.flags.contains(THEME_SYNC)`, call `apply_theme(&palette).await` on each, log per-adapter errors at `warn!` (never propagate). Add a unit test stubbing two adapters (one supports, one doesn't) and asserting only the supporter is called.
- [ ] 2.2 Add Tauri command `apply_theme_to_agents(palette: ThemePalette) -> Result<(), String>` in `src-tauri/src/commands.rs` that forwards to the registry. Map errors to strings. Register the command in `src-tauri/src/lib.rs` (`invoke_handler` macro).
- [ ] 2.3 Wire frontend invocation in `src/App.tsx`: after the existing `applyTheme(themeMode, customThemes)` effect, dispatch `requestAnimationFrame` → read computed CSS tokens via a new helper `extractPaletteFromComputedStyle()` in `src/lib/themes.ts` → debounce 150ms trailing → invoke `apply_theme_to_agents`. Failures are silent (console.warn only).
- [ ] 2.4 Add `extractPaletteFromComputedStyle(): ThemePalette` helper in `src/lib/themes.ts`. Reads `--terminal-surface`, `--terminal-foreground`, `--card`, `--secondary`, `--muted-foreground`, `--border`, `--primary` from `document.documentElement` computed style. Computes `isDark` from surface luminance. Resolves `id` from `document.documentElement.dataset.theme`.

## Phase 3 — pi adapter

- [ ] 3.1 Add `apply_theme` impl in `src-tauri/src/agents/pi/adapter.rs`. Build a 51-token theme JSON from the palette (use a private `build_pi_theme(palette: &ThemePalette) -> serde_json::Value` helper). Write atomically to `~/.pi/agent/themes/cluihud-active.json` (write to `.cluihud-active.json.tmp` then rename). Use `dirs::home_dir()` + `tokio::fs`.
- [ ] 3.2 Implement settings.json reconciliation: read `~/.pi/agent/settings.json` (or treat as `{}` if missing); only set `"theme": "cluihud-active"` if absent OR already `"cluihud-active"`. Preserve other keys. Write atomically.
- [ ] 3.3 Update `OpenCodeAdapter::new`-style constructor for pi to declare `AgentCapability::THEME_SYNC` in the capabilities bitset (see `src-tauri/src/agents/pi/adapter.rs` `Self::new`).
- [ ] 3.4 Add unit tests in `pi/adapter.rs`:
  - `pi_apply_theme_writes_expected_json`
  - `pi_apply_theme_preserves_existing_settings_keys`
  - `pi_apply_theme_does_not_overwrite_user_theme_choice`
  - `pi_build_theme_emits_all_51_required_tokens`
- [ ] 3.5 Update spec delta in `specs/pi-adapter/spec.md`.

## Phase 4 — opencode adapter

- [ ] 4.1 Spike before coding: spin up opencode TUI manually, write a sample custom theme `~/.config/opencode/themes/spike-test.json`, set `tui.json` to `"theme": "spike-test"`, restart opencode, confirm theme applies. Then test `curl -X POST http://127.0.0.1:<port>/tui/execute-command -d '{"command":"theme spike-test"}'` against a running opencode — confirm whether live switch works. Record verdict in `handoff/spike-opencode-live-switch.md` (created during impl).
- [ ] 4.2 Add `apply_theme` impl in `src-tauri/src/agents/opencode/adapter.rs`. Build the opencode theme JSON via private `build_opencode_theme(palette: &ThemePalette) -> serde_json::Value` using the schema `$schema: "https://opencode.ai/theme.json"` with `defs` + `theme.dark`/`theme.light` blocks. Write atomically to `~/.config/opencode/themes/cluihud-active.json`.
- [ ] 4.3 Implement tui.json reconciliation analogously to pi (set `"theme": "cluihud-active"` only if absent or already ours).
- [ ] 4.4 If the spike from 4.1 confirms live-switch works: iterate `session_ports` map, POST `/tui/execute-command {"command": "theme cluihud-active"}` to each. 1.5s timeout per request, log + ignore failures. If the spike says NO: skip the HTTP step entirely and document in implementation.md why.
- [ ] 4.5 Update opencode adapter `Self::new` to advertise `AgentCapability::THEME_SYNC`.
- [ ] 4.6 Add unit tests:
  - `opencode_apply_theme_writes_expected_json`
  - `opencode_apply_theme_preserves_existing_tui_json_keys`
  - `opencode_apply_theme_does_not_overwrite_user_theme_choice`
  - `opencode_build_theme_includes_required_token_set`
- [ ] 4.7 Update spec delta in `specs/opencode-adapter/spec.md`.

## Phase 5 — codex adapter

- [ ] 5.1 Add `apply_theme` impl in `src-tauri/src/agents/codex/adapter.rs`. Parse `~/.codex/config.toml` (use the `toml` crate already in the workspace if present; otherwise treat as text edit limited to the `[tui]` table). Set `theme = "<derived>"` based on `palette.is_dark` (mapping resolved during impl spike — see implementation.md). Write atomically.
- [ ] 5.2 Update codex adapter `Self::new` to advertise `AgentCapability::THEME_SYNC`.
- [ ] 5.3 Add unit tests:
  - `codex_apply_theme_writes_tui_theme_key`
  - `codex_apply_theme_preserves_other_config_keys`
  - `codex_apply_theme_handles_missing_config_file`
- [ ] 5.4 Update spec delta in `specs/codex-adapter/spec.md` with the documented "syntax-highlighting only, next-spawn" limitation.

## Phase 6 — Verification

- [ ] 6.1 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.
- [ ] 6.2 Run `npx tsc --noEmit`.
- [ ] 6.3 Manual walk per the proposal Build contract `Cómo verifico` step. Capture findings (especially opencode live-switch result) in `handoff/manual-verification.md`.
- [ ] 6.4 If opencode live switch is confirmed: document in CHANGELOG that opencode follows live now. If not: document the next-spawn behavior so users know what to expect.

## Phase 7 — Docs

- [ ] 7.1 Add a one-paragraph note in `docs/architecture.md` describing the theme-sync data flow.
- [ ] 7.2 Add a CHANGELOG entry in `Obsidian23/Projects/nergal/Changelog.md` (handled by /openspec-sync post-archive, NOT during this change).
