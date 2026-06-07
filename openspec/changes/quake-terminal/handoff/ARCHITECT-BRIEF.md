# Architect brief — Quake terminal

**Project mission**: Nergal is a Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. Nergal runs *around* the agent — observes and augments, never reimplements the agent's primitives.

## Context

Crystallized from a design conversation (2026-06-06), not a `/discovery` interview — the conversation WAS the discovery. Full decision log in `design.md`; durable anchor in memory `project_quake_terminal.md`. Status: artifacts created, **implementation deferred to a fresh session**.

## Sprint Contract

See `proposal.md` § Build contract. Tier **L** (multi-module, schema/migration, phased into 5 independently-verifiable steps). risk_tier: medium (additive migrations; no auth/billing/security surface).

## Why this is low-risk despite being L

The backend already has the hard part: a session terminal is a shell PTY into which the agent command is *written*. An aux shell is the same minus that write. The renderer is already agent-agnostic. The only structural refactor is `terminalService` single-host → per-region hosts, which is bounded and back-compat (default region = `center`).

## Dependencies / blockers

- None external. Self-contained within the existing PTY + renderer + DB + settings patterns.
- Reuses established per-workspace config pattern (openspec-dir override, obsidian config) for suggestions.
- The `ports` status-bar chip surfaces dev servers for free (no wiring).

## Risk tier + gating

- risk_tier: medium. tags: [feature, migration]. files_estimate: 14.
- Migrations are additive (new column/table for env-shell defs + suggestions). No data loss path.
- Gating recommendation when implemented: standard verify (clippy -D warnings, cargo test, fmt, tsc) per phase; manual UX walk before archive. No security/auth review needed (no such surface).

## Implementation order (from tasks.md)

1. Foundation (per-region renderer + agentless shell PTY) — unblocks everything.
2. Quake overlay UI + focus zone — usable with ad-hoc shells.
3. Environment shells (modal + persistence) — the footgun fix lands here.
4. Per-workspace suggestions.
5. Polish (prelude reframe) + specs reconcile + full check + UX walk.

## Watch-outs for the builder

- `Ctrl+}` binding: the `shortcuts.ts` registry mixes key-char (`ctrl+ñ`) and code-based matching — confirm the path for `BracketRight`+Shift before assuming a format. CLAUDE.md mandates `event.code`.
- Keep the agent terminal path byte-identical through the `center` region (don't regress the existing single-terminal flow).
- Per-session teardown must kill aux shells (no orphan PTYs).
- Distinguish first-creation (auto-run) vs re-open (pre-fill) for env shells — this is a deliberate behavior difference.
- Prelude stays in the agent terminal (the agent must inherit its env); do NOT route preludes to quake shells.
