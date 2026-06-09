# ARCHITECT-BRIEF — cross-session-messaging

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
Second of the three MCP-first changes. Adds autonomous agent-to-agent messaging over the MCP daemon, supporting the transitive A→B→C case, replacing the messaging half of the archived `context-bridge` (file-bus + CAMBIO/CONSENSO).

## Sprint Contract
See `../proposal.md` § Build contract. Summary: `send_to_session`/`read_messages`/`list_threads`/`search_sessions` tools; migration-backed store (`015_cross_session`, `agent_consumed_at` vs `human_seen_at` separated); hybrid delivery (idle PTY wake / working `cluihud hook stop` CLI stdout `additionalContext`) with an idle-transition drain owned by the mode-map writer; non-authoritative posture = labeling + documented limits only (no provenance gate — unattributable); per-message reach hop cap + dedup-status + count/time budget with an active deadline sweeper; kill-switch default-off; right-panel history + `human_seen` badge.

## Dependencies / blockers
- **Depends on** `cluihud-mcp-server` (daemon, directory, identity, transport).
- Provides the `SessionDelivery` channel reused by `agent-spawned-worktrees`.

## Risk tier
critical (autonomous PTY injection into live sessions; security). Default-off kill-switch.

## Gating decision
Triple-prompt gating ON. iterative-plan-review (Claude evaluator): **4 rounds → APPROVED**. Security reviewer at build time for the delivery + non-authoritative phases (PTY injection surface).
