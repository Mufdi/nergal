# ARCHITECT-BRIEF — cluihud-mcp-server

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
Foundation of the MCP-first replacement for the archived `context-bridge`. First time Nergal exposes its observed state back to agents (a session directory) via an MCP daemon. Standalone value + base for `cross-session-messaging` and `agent-spawned-worktrees`.

## Sprint Contract
See `../proposal.md` § Build contract (Qué construyo / Cómo verifico / Criterio de done / Estimated scope). Summary: MCP daemon + dedicated length-framed JSON-RPC transport (uid-restricted 0600 socket) + `cluihud mcp` stdio shim; cooperative env-hint identity validated against the live registry (NO pid-walk — TOCTOU theater); global-read-within-uid directory (`whoami`/`list_sessions`/`get_session`) with snapshot-then-release lock discipline; additive Stop-hook bg-tasks/crons; opt-in net-new AI summarizer (dedicated key, SQLite migration 014); idempotent registration pinning `/usr/bin/cluihud`.

## Dependencies / blockers
- None upstream. Two downstream changes depend on this: `cross-session-messaging`, `agent-spawned-worktrees`.
- New transport is required because the existing hook socket is fire-and-forget (`server.rs:205-261`).

## Risk tier
critical (security surface: uid boundary + cross-workspace data exposure). Default-off until opted in.

## Gating decision
Triple-prompt gating ON (risk critical, files_estimate 20, tags security). iterative-plan-review run (Claude evaluator): **5 rounds → APPROVED**. Security/build reviewers warranted at build time for the transport framing + identity + summarizer phases.
