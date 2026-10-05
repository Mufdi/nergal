Linear: FRI-498

## Why

Nergal's panels (Activities, Tasks, plan-review, ask-user) only work once the `nergal hook …` commands are registered in `~/.claude/settings.json`. The registrar exists (`setup::run()`, exposed as the `nergal hook setup` CLI and the `setup_hooks` Tauri command), but nothing in the app calls it. A fresh install therefore has dead panels until the user finds a hidden CLI command.

Scope is deliberately minimal (re-validated 2026-10-05). The focus freeze targets Linux + Claude Code, where the normal case is "everything already installed and working". This change only closes the fresh-install gap. It does not build a general onboarding flow.

## What Changes

- **Hooks status probe**: a new read-only backend command reports whether the Nergal hooks are registered, the settings file path, and whether `nergal` is on `PATH`. It shares the registrar's merge logic, so "registered" means "setup would change nothing".
- **PATH guard**: Unix hooks call bare `nergal`. On an AppImage install it is not on `PATH`, so the dialog disables "Configure" and explains why.
- **Hooks setup dialog**: at app launch, when the probe reports the hooks as not registered, a dialog opens. It names the file it will modify and offers two actions: "Configure" and "Skip".
- **Consented write**: "Configure" calls the existing idempotent `setup_hooks` command. The app never writes `~/.claude/settings.json` without that click.
- **Skip is per launch**: "Skip" closes the dialog for the current launch. The next launch probes again. No persisted flag exists.
- **Removed from the original plan**: the multi-step wizard, the `onboarding_completed` config flag, the status-bar health chip, the Settings re-entry, the agent/shell/transcripts review, the keyboard-ownership step and the per-OS notes.

## Capabilities

### New Capabilities
- `setup-wizard`: detect at launch whether the Nergal hooks are registered in Claude Code's settings, and register them with one consented click.

### Modified Capabilities
<!-- None. The dialog calls the existing setup_hooks primitive; no existing spec requirement changes. -->

## Impact

- **Backend** (`src-tauri/src/`): `setup.rs` gains a read-only status function that reuses the merge logic of `setup::run()`. A new Tauri command exposes it and is registered in `lib.rs`. No `Config` change.
- **Backend, side fix**: the registrar now errors on a hook event value that is not an array, instead of skipping it silently. `setup_hooks` reports the full error chain.
- **Frontend** (`src/`): one new dialog component, mounted at the app root next to `<WorktreeGate/>`. One new TS type and one pure helper for the probe result.
- **User-facing**: a fresh install reaches live panels after one click. Installs that already have the hooks never see the dialog. If a user later removes the hooks, the dialog appears again on the next launch.
- No SQLite schema change. No new dependencies. No network calls.
