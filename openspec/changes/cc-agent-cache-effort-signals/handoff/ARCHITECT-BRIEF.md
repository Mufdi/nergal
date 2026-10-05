# Architect brief: cc-agent-cache-effort-signals

**Project mission**: Nergal is a Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

**Tracker**: Linear FRI-501 (team RR, project Nergal, milestone "1 · Terminal y sesiones").

## Control metadata

| Field | Value |
|---|---|
| tier | L |
| ceremony | deep |
| risk_tier | medium |
| files_estimate | 14 |
| tags | feature, hooks, status-bar |
| spec_target | agent-status-telemetry (new), session-state-indicators, activity-timeline, cc-adapter |

## Context

- Payload shapes verified against code.claude.com docs (hooks.md, statusline.md) and local CC 2.1.289 transcripts. See design.md Context table.
- `agent_needs_input` / `agent_completed` are background-session (agent view) signals, not in-session subagent signals. In-session subagents are covered by `SubagentStart` / `SubagentStop`.
- No parent agent id exists in hook payloads: nested trees are out of scope.

## Dependencies / blockers

- FRI-498 (another session) refactors `setup.rs::run()` on main. This change touches only the `HOOKS` table and the tests module of `setup.rs`.

## Gating decision

Triple-prompt gating ON (`files_estimate >= 5`). Per-step reviewer: single-sequential. Once-per-change: frontend review (UI diff) + adversarial (tier L).

## Lazy-skill check (A6)

UI work is two status-bar chips and one activity type that copy existing mechanics (`StatusBar` context chip, `completeToolActivityAtom`). No lazy-library skill adds guidance beyond `docs/design.md` §1.2 and §3.7 (tooltips).
