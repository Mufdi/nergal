# First-run setup wizard

## Why

Nergal augments the agent loop through the hook pipeline: Activities, Tasks, plan-review and ask-user panels only come alive once the `nergal hook …` commands are registered in `~/.claude/settings.json`. That registration exists (`setup::run()`, exposed as the `nergal hook setup` CLI subcommand **and** the `setup_hooks` Tauri command registered in `lib.rs`) but **nothing ever triggers it**: there is no call from the frontend, no startup invocation, and no documentation. On a fresh install — any OS — the panels are silently dead until the user runs a hidden CLI command they have no way to discover. The gap is universal but surfaces first on a freshly-installed OS (the Windows port walk hit it: `%USERPROFILE%\.claude\settings.json` was never written). There is likewise no surface that tells the user "no agent CLI is on your PATH" — the other precondition for the loop to work at all.

## What Changes

- **New first-run wizard**: on the first launch (tracked by a new `onboarding_completed` config flag), a guided, skippable, non-blocking wizard opens. It runs a set of readiness checks and offers one-click fixes / suggestions, matching the app's existing overlay style and conventions.
- **Backend readiness probe**: a new `setup_status` Tauri command reports, in one call, whether hooks are registered, whether ≥1 agent CLI is detected on PATH, the resolved default shell, and the transcripts directory — so the wizard (and the health-gate) render from a single source of truth.
- **One-click hooks configuration (consented)**: the wizard detects the hook state and offers a visible "Configure" action that invokes the existing `setup_hooks` (idempotent). Out-of-the-box, but the user sees what will be touched before `~/.claude/settings.json` is modified — never a silent mutation of their global Claude Code config.
- **Non-blocking health-gate**: after first run, a non-blocking banner/indicator (in the status bar surface) reappears when a **critical** check regresses (hooks removed, or no agent on PATH), with a one-click path back into the wizard. The wizard is also re-openable on demand from Settings.
- **Agent / shell / transcripts review**: the wizard surfaces the detected default agent (pick when several are installed), the resolved shell (`resolve_pty_shell`), and the transcripts directory as suggestions — critical checks (hooks + agent) gate the "all set" state; the rest are informational.
- **Platform-aware notes**: a per-OS note card (SmartScreen on Windows, Gatekeeper on macOS, launcher-PATH on Linux) rendered from the existing `installSource`, reusing the About-panel pattern.
- **Settings re-entry**: a "Run setup again" entry in the Settings panel re-opens the wizard.

No agent primitives are reimplemented — the wizard observes and wires the existing hook pipeline + agent registry. This keeps the change inside Nergal's scope ("surfaces around the agent ecosystem", reducing friction in the agent↔human loop).

## Capabilities

### New Capabilities
- `setup-wizard`: first-run readiness flow + persistent health-gate that detect Nergal's runtime prerequisites (hooks registered, agent on PATH, shell, transcripts) and drive one-click / consented fixes so a fresh install works out-of-the-box.

### Modified Capabilities
<!-- None. The wizard calls the existing internal setup_hooks primitive and the agent registry; no spec-level behavior of an existing capability changes. -->

## Impact

- **Backend** (`src-tauri/src/`): new `setup_status` command (+ registration in `lib.rs`); a new `onboarding_completed: bool` field on `Config` (`config.rs`) gated out of the frontend-writable keys via `BACKEND_OWNED_CONFIG_KEYS` only if it must stay backend-owned (TBD in design — it is a simple persisted flag). Reuses `setup::run()`/`setup_hooks`, `agents::registry::AgentRegistry::scan` → `DetectionResult.installed`, and `config::resolve_pty_shell`.
- **Frontend** (`src/`): a new wizard overlay component (matching the app's modal/overlay pattern), a status-bar health-gate indicator, a first-run trigger at app mount, a Settings re-entry, and the `Config`/types plumbing for the new flag. Reuses the agent store (`installed` + `agents:detected`), `installSource`, and `lib/confirm.ts`.
- **User-facing**: a fresh install reaches a working state (live panels) without any hidden CLI step. Existing installs already configured are unaffected (the wizard's checks pass; `onboarding_completed` is set on first completion/skip).
- **No SQLite schema changes.** No new dependencies. No external network calls.
