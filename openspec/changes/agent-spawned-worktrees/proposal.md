## Why

The cross-session workflow has a natural endpoint: A and B converse and reach consensus that some implementation work must happen, e.g. "the front needs change X". To avoid polluting the current session, the agent decides the right move is a fresh worktree session dedicated to that task. Today only the user can create a worktree session.

This change lets an agent *request* the creation of a worktree session under an active workspace, with a dedicated initial prompt — but always behind a mandatory human confirmation gate. After the user approves and the initial prompt is sent, control passes to the user, who owns that session thereafter (keeps it or deletes it later). It is the most resource-sensitive capability of the set, so it ships last, on top of `cluihud-mcp-server` and after `cross-session-messaging`.

It folds the backlog item "Workspace presets con scope concreto" (setup/teardown via the existing `session-launch-options`) so a spawned worktree can run its project's setup automatically.

## What Changes

- New MCP tool `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` that does NOT create immediately — it raises a **confirmation request** to the user.
- **Mandatory human gate**: cluihud surfaces a confirmation UI ("Session X wants to create a worktree under workspace W with this prompt: …  [Approve / Edit / Deny]"). The gate cannot be bypassed by auto-mode (resource/irreversible action; mirrors CC v2.1.166 non-authority of relayed requests + RULES "pause on ambiguity even in auto-mode").
- On **approve**: cluihud creates the worktree session via the existing worktree machinery + `session-launch-options` (presets), injects the dedicated initial prompt as the session's first turn, then **hands control to the user**. cluihud does not drive the new session further.
- On **edit**: the user may adjust the prompt/branch before approving.
- On **deny**: the tool returns a denied result to the calling agent; nothing is created.
- **Lifecycle**: aligns with Claude-managed worktrees (CC v2.1.157 leaves them unlocked when the agent finishes, enabling `git worktree remove`/`prune`). The user owns cleanup; cluihud does not auto-delete an approved worktree.

## Capabilities

### New Capabilities
- `agent-spawned-worktrees`: The `create_worktree_session` MCP tool, the mandatory human confirmation gate (un-bypassable by auto-mode), the dedicated-initial-prompt creation flow with `session-launch-options` integration, the hand-off-to-user transfer of control, and the worktree lifecycle alignment.

### Modified Capabilities
<!-- Builds on cluihud-mcp-server. Reuses existing worktree creation + session-launch-options. No existing spec-level behavior changes. -->

## Impact

- **Backend**: extend `src-tauri/src/mcp/` with the `create_worktree_session` tool; a pending-request queue + Tauri command for approve/edit/deny; wire approval into existing worktree creation + `session-launch-options`.
- **Frontend**: confirmation UI (modal or actionable toast) showing requesting session, target workspace, branch, prompt, and launch options, with Approve/Edit/Deny.
- **File system**: worktrees created under the existing `<repo>/.worktrees/` convention.
- **Existing flows**: reuses worktree creation and launch options; adds an agent-initiated entry point gated by human confirmation.

## Build contract

### Qué construyo
- MCP tool `create_worktree_session` that raises a confirmation request (never creates directly).
- Mandatory human gate UI (Approve/Edit/Deny), un-bypassable by auto-mode.
- Approval flow: create worktree session via existing machinery + `session-launch-options`, inject dedicated initial prompt, hand control to the user.
- Deny/edit handling + structured tool results back to the calling agent.
- Lifecycle alignment with Claude-managed worktrees; no cluihud auto-delete.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual: an agent calls `create_worktree_session`; confirm the gate appears and nothing is created until approval; approve and confirm a new worktree session starts with the dedicated prompt and presets, then control is the user's; deny and confirm nothing is created and the agent gets a denied result; confirm auto-mode does not bypass the gate.

### Criterio de done
- No worktree is ever created without explicit user approval, including in auto-mode.
- On approval, a worktree session starts under the target workspace with the dedicated initial prompt and project presets, then control passes to the user.
- On deny, nothing is created and the agent receives a structured denied result.
- The created worktree follows the existing `.worktrees/` lifecycle; cluihud never auto-deletes it.

### Estimated scope
- files_estimate: 9
- risk_tier: critical
- tags: [feature, security]
- visibility: public
- spec_target: agent-spawned-worktrees
