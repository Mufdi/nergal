# ARCHITECT-BRIEF — clickup-task-integration

## Project mission
Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

## Context
Second of three chained ClickUp changes. Turns a mirrored task into something the agent acts on: three verbs (send-as-prompt, spawn-worktree, attach-as-context) plus a 1:1 session↔task binding. The whole point is REUSE — it rides the existing Obsidian context-injection machinery (`injected_context` assembler at `pty.rs:396` + the `ContextInjection` adapter contract), the `pending_prompts` initial-prompt path, the `reinject_pinned_note` live-PTY write, and the user-initiated `create_worktree`. No new injection plumbing, no new adapter variant.

## Sprint Contract
See `../proposal.md` § Build contract. Summary: migration adds `active_clickup_task_id` + `pinned_clickup_task_ids` to `sessions`; `compose_task_markdown` from the mirror; extend the `pty.rs:396` assembler to concatenate ClickUp context; commands for the 3 verbs + bind/unbind/pin/unpin/reinject; frontend actions + session-tab active-task indicator.

## Dependencies / blockers
- **Depends on** `clickup-sync` (mirror = task source, panel = action surface). Migration number = next free after clickup-sync's. Independent of the context-bridge changes.
- The worktree verb uses the **user-initiated** machinery (`worktree.rs:261`), NOT the `agent-spawned-worktrees` context-bridge change.

## Risk tier
critical (migration on `sessions`; edits the spawn hot path at `pty.rs:396`; live-PTY write must not interrupt a mid-stream agent).

## Gating decision
Triple-prompt gating ON (risk_tier critical, migration tag). iterative-plan-review (Claude evaluator) planned pre-build. Reviewer focus at build: the `pty.rs:396` hot-path edit (lock discipline, byte-identical-when-empty), the live-PTY mid-stream guard, and binding/rebind correctness.
