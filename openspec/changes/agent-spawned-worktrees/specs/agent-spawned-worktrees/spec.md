## ADDED Requirements

### Requirement: create_worktree_session tool requests, never creates

The system SHALL expose a `create_worktree_session(workspace_id, prompt, branch_name?, agent?, launch_options?)` MCP tool that enqueues a pending creation request and returns a pending handle to the calling agent. The tool SHALL NOT create a worktree or session synchronously.

#### Scenario: Tool raises a request

- **WHEN** an agent calls `create_worktree_session` with a valid active workspace and a prompt
- **THEN** the system SHALL enqueue a pending creation request and return a pending handle, and SHALL NOT create anything yet

#### Scenario: Invalid target workspace

- **WHEN** an agent calls `create_worktree_session` with a workspace that is not active/known
- **THEN** the tool SHALL return a structured error and SHALL NOT enqueue a request

### Requirement: Mandatory human confirmation gate

The system SHALL surface a confirmation UI for each pending creation request, showing the requesting session, target workspace, branch, the dedicated prompt, and launch options, with Approve / Edit / Deny actions. The gate SHALL NOT be bypassable by auto-mode.

#### Scenario: Nothing created before approval

- **WHEN** a creation request is pending
- **THEN** no worktree and no session SHALL exist until the user approves

#### Scenario: Auto-mode does not bypass the gate

- **WHEN** auto-mode is active and a creation request is raised
- **THEN** the system SHALL still require explicit user approval and SHALL NOT auto-approve

#### Scenario: User edits before approving

- **WHEN** the user chooses Edit on a pending request
- **THEN** the user SHALL be able to adjust the prompt and/or branch before approving

#### Scenario: User denies

- **WHEN** the user chooses Deny
- **THEN** nothing SHALL be created and the calling agent SHALL receive a structured denied result

### Requirement: Approval creates via existing machinery and hands off

On approval, the system SHALL create the worktree session through the existing worktree creation machinery and `session-launch-options` (running project presets/setup), inject the dedicated prompt as the new session's first turn, and then transfer control to the user. The system SHALL NOT send further prompts to the new session.

#### Scenario: Approved spawn runs the dedicated prompt with presets

- **WHEN** the user approves a creation request
- **THEN** a new worktree session SHALL start under the target workspace with project presets applied, and the dedicated prompt SHALL be delivered as its first turn

#### Scenario: Control passes to the user

- **WHEN** the dedicated initial prompt has been delivered
- **THEN** control of the new session SHALL belong to the user and cluihud SHALL NOT drive it further

### Requirement: Worktree lifecycle alignment, no auto-delete

The created worktree SHALL follow the existing `<repo>/.worktrees/` convention and the Claude-managed unlock-on-finish behavior so standard `git worktree remove`/`prune` works. The system SHALL NOT auto-delete an approved worktree; the user owns its removal.

#### Scenario: User owns cleanup

- **WHEN** an approved worktree session is later idle or finished
- **THEN** the system SHALL NOT auto-delete it, and it SHALL remain removable by the user via standard worktree tooling
