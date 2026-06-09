# ARCHITECT-BRIEF — clickup-writeback

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
Third and last of the chained ClickUp changes. Closes the loop: full bidirectional editing (status, comments, checklist toggles, description/assignees/due) reflected upstream, plus the integration payoff — an explicit write-back-on-done closure when a session bound to a task reaches `Completed`. The hard parts are the sync concerns (echo dedup, conflict) and keeping every outward-facing write human-confirmed.

## Sprint Contract
See `../proposal.md` § Build contract. Summary: write methods on the existing client; optimistic apply + `pending_writes` registry + rollback; echo dedup + last-writer-wins conflict by `date_updated` in the poller reconcile; explicit closure hooked on `SessionStatus → Completed` for bound sessions; frontend write controls + closure prompt + echo-silent/conflict-warn toasts.

## Dependencies / blockers
- **Depends on** `clickup-sync` (mirror + client + poller) AND `clickup-task-integration` (the `active_clickup_task_id` binding that scopes the closure). Ships last. No migration. Independent of the context-bridge changes.

## Risk tier
critical (security: every write is outward-facing, visible to the user's team; the closure must be un-auto-able / human-confirmed).

## Gating decision
Triple-prompt gating ON (risk_tier critical, security tag). iterative-plan-review (Claude evaluator) planned pre-build. Security reviewer at build time for the write paths + the closure (verify no auto-write path exists) + echo/conflict ordering (own-write must never self-notify).
