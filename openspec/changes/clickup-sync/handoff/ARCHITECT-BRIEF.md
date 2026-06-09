# ARCHITECT-BRIEF — clickup-sync

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
First of three chained ClickUp changes. Builds the data + read foundation: token auth (keyring), a typed ClickUp REST client, a structure-agnostic SQLite mirror of the hierarchy + tasks, a polling sync engine with change detection, and a read-only Linear-style panel. Delivers value alone (read your ClickUp tasks inside Nergal, with assignment notifications). Runs **parallel** to the context-bridge MCP-first track — no dependency.

## Sprint Contract
See `../proposal.md` § Build contract. Summary: keyring token storage + disk fallback; `reqwest`-based client (rate-limit + pagination); 11-table mirror modeling the generic `Space→Folder→List→Task→Subtask` tree with statuses-per-list + custom-field defs/values as rows (NEVER enums); poller + diff (moved-task detection, tombstoning, lazy heavy sub-data, new-assignment `notify-send`); read-only panel with persistent Space selector + group-by + assigned-to-me + floating detail module.

## Invariante (load-bearing)
ClickUp structure (tree shape, per-list statuses, custom fields, types) is **runtime-synced data, never code constants**. Adding a Folder/List/status/field must require zero code change and zero migration. The current workspace snapshot validated payload shapes only.

## Dependencies / blockers
- None. No dependency on the context-bridge changes. **Migration numbering**: assign next free (≥015) at build time; the 014/015 numbers in the context-bridge specs are stale — do not collide.

## Risk tier
critical (migration + security: a Personal API token grants full account access; stored in keyring, never logged, never sent to frontend).

## Gating decision
Triple-prompt gating ON (risk_tier critical, files ~22, tags migration+security). iterative-plan-review (Claude evaluator) planned pre-build. Security reviewer at build time for the auth + token-handling phases; deps reviewer for the `keyring` addition.
