# ARCHITECT-BRIEF — agent-spawned-worktrees

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
Third and most resource-sensitive of the MCP-first changes. Lets an agent REQUEST (never directly create) a worktree session under an active workspace, behind a mandatory human gate — the natural endpoint of a cross-session consensus.

## Sprint Contract
See `../proposal.md` § Build contract. Summary: `create_worktree_session` (non-blocking, returns `pending_request_id`) + `get_worktree_request_status` + `cancel_worktree_request`; two-structure model (pending queue + terminal-status ledger with TTL) so atomic-purge and pollable-status coexist; outcome via `cross-session-messaging` SessionDelivery (push) + poll (pull); structurally un-bypassable human gate (sole entry = `approve_worktree_request` Tauri command; `agent`+`permission_preset` surfaced explicitly); approval = slug-collision check → `create_worktree` → existing `LaunchOptions` (no setup-runner — Workspace presets is a separate unimplemented backlog item) → dedicated first prompt via `pending_prompts` → hand off; spawn-failure rollback; kill-switch default-off; restart → `not_found` (abandoned).

## Dependencies / blockers
- **Depends on** `cluihud-mcp-server` (MCP) AND `cross-session-messaging` (the `SessionDelivery` outcome channel). Ships last.

## Risk tier
critical (resource/irreversible session creation; security — escalation surface via agent-chosen permission preset). Default-off.

## Gating decision
Triple-prompt gating ON. iterative-plan-review (Claude evaluator): **5 rounds → APPROVED**. Security reviewer at build time for the gate + escalation-surface + rollback phases.
