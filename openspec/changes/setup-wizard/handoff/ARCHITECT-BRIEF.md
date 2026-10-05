# Architect Brief — setup-wizard

## Project mission
Nergal — Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. Nergal runs around the agent; it does not reimplement agent primitives.

## Control metadata (from `.work-modules.json`)
- **Tier**: M · **Ceremony**: standard (+ implementation.md kept) · **Risk**: medium
- **Files estimate**: 7 · **Tags**: feature, onboarding · **Spec target**: `setup-wizard` (new)
- **Tracker**: Linear FRI-498

## Context
Re-validated 2026-10-05. The user cut the scope to: on launch, if the Nergal hooks are not registered, show a dialog that configures them with one consented click. No flag, no wizard steps, no health chip, no Settings entry, no per-OS notes. See `design.md` D1-D7.

## Gating decision
- Triple-prompt gating: ON (`files_estimate >= 5`), single-reviewer sequential per group.
- Once per change: frontend reviewer (UI diff), adversarial reviewer (tier M).
- Gates: compile + test + lint + placeholders. No security/deps gates (no auth, no new deps).
- iprev: 2 rounds, APPROVED.

## Lazy-skill notes (A6)
UI group: `/frontend-build` Steps 2-3b once, summary in `handoff/ui-brief.md`. Follow `docs/design.md` tokens and copy `ProjectBootstrapPrompt` (layout) + `ConfirmHost` (keyboard) exactly. `/frontend-review` runs as the frontend reviewer.
