## Context

cluihud already creates worktree sessions on user action: it owns the worktree machinery (`<repo>/.worktrees/`, slug generation) and `session-launch-options` (per-session launch config, including setup commands). `cluihud-mcp-server` gives agents a tool surface and identity; `cross-session-messaging` produces the consensus that motivates a spawn. This change adds the agent-initiated entry point — but creating a session is a resource-consuming, hard-to-reverse action, so it is gated.

CC's worktree handling informs the lifecycle: v2.1.157 leaves Claude-managed worktrees unlocked when the agent finishes so `git worktree remove`/`prune` can clean them, and `EnterWorktree` can switch between managed worktrees mid-session. The security precedent (v2.1.166: relayed cross-session requests carry no user authority) means an agent's spawn request is a *request*, never an authorization.

## Goals / Non-Goals

**Goals:**
- Let an agent propose a dedicated worktree session as the natural endpoint of a cross-session consensus.
- Make the human confirmation gate mandatory and un-bypassable by auto-mode.
- Reuse existing worktree creation + `session-launch-options` (fold "Workspace presets").
- Hand control to the user immediately after the initial prompt — cluihud does not autopilot the new session.

**Non-Goals:**
- Autonomous, ungated session creation (explicitly rejected — resource/irreversible).
- cluihud driving the spawned session beyond its first prompt.
- Auto-deletion/cleanup of approved worktrees (the user owns lifecycle).
- Reviving inactive sessions in place (a spawn creates a fresh worktree session; resuming a specific past session is out of scope here).

## Decisions

### 1. The tool requests; it never creates

**Decision**: `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` enqueues a pending creation request and returns a pending handle to the agent. It does not create anything synchronously.

**Why**: Creation must pass through the human gate. A tool that created directly would be an ungated resource action and a privilege-escalation path from a relayed message.

### 2. Mandatory human gate, un-bypassable by auto-mode

**Decision**: cluihud surfaces a confirmation UI showing the requesting session, target workspace, branch, the dedicated prompt, and launch options, with **Approve / Edit / Deny**. Auto-mode SHALL NOT auto-approve it.

**Why**: Creating a session is in the "resource / irreversible" class that RULES requires pausing on even in auto-mode, and CC v2.1.166 establishes that cross-session requests carry no user authority. This is the security spine of the change (critical tier).

**Edit**: the user can adjust the prompt or branch before approving — useful when the agent's proposed scope is close but not exact.

### 3. Approval reuses existing creation + launch options; then hands off

**Decision**: On approve, cluihud creates the worktree session through the existing worktree machinery and `session-launch-options` (so project presets / setup commands run), injects the dedicated prompt as the new session's first turn, and then transfers control to the user. cluihud does not send further prompts to the new session.

**Why**: Reuse keeps behavior consistent with user-created worktrees and folds the "Workspace presets" backlog item without a parallel path. Handing off matches the user's intent: the spawned session is theirs to continue or discard.

### 4. Lifecycle aligns with Claude-managed worktrees; no auto-delete

**Decision**: The created worktree follows the existing `<repo>/.worktrees/` convention and the Claude-managed unlock-on-finish behavior (v2.1.157), so standard `git worktree remove`/`prune` works. cluihud SHALL NOT auto-delete an approved worktree; the user decides when to remove it.

**Why**: Ownership transferred to the user at hand-off (Decision 3); auto-deleting their session would be surprising and destructive. Aligning with CC's worktree lifecycle avoids fighting the agent's own cleanup.

## Risks / Trade-offs

**[Risk] Agent spam-requests session creation** → Every request hits the human gate; nothing is created without approval. Optionally rate-limit pending requests per requesting session (deny with a structured "too many pending requests" error) to keep the gate UI usable.

**[Risk] Relayed message coerces a spawn** → The request carries no authority (Decision 2); the user sees the requesting session and the full prompt before approving, so a coerced or malicious prompt is visible and deniable. Security review required.

**[Risk] Resource exhaustion from many approved worktrees** → The user approves each one, so growth is human-paced. The existing worktree tooling + this change's lifecycle alignment keep cleanup standard.

**[Trade-off] Hand-off vs autonomous follow-through** → By design cluihud stops after the first prompt. The spawned session does not auto-report back into the originating thread; if the user wants that, they can wire the new session into a cross-session thread manually. Accepted to keep the autonomy boundary clear.
