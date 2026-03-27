## ADDED Requirements

### Requirement: Create session with git worktree
The system SHALL create a new session by generating a git worktree at `<repo>/.worktrees/cluihud/<slug>/` with branch `cluihud/<slug>`. The first session in a workspace MAY use the main checkout without a worktree.

#### Scenario: Create second session in workspace
- **WHEN** user clicks "+" on a workspace that already has one session
- **THEN** system creates a git worktree branching from the current HEAD
- **AND** creates a PTY with cwd set to the worktree path
- **AND** writes `claude` to the PTY to start Claude Code
- **AND** the new session appears in the sidebar as "Running"

#### Scenario: Create first session in new workspace
- **WHEN** user creates the first session in a workspace
- **THEN** system uses the main repository checkout path as cwd (no worktree)
- **AND** creates a PTY and starts Claude Code

### Requirement: Switch session
The system SHALL allow switching between sessions by clicking on a session in the sidebar. Switching SHALL swap all visible state (terminal, plan, tasks, activity, files, cost) to the selected session's context.

#### Scenario: Switch from session A to session B
- **WHEN** user clicks session B in the sidebar while session A is active
- **THEN** terminal A becomes hidden, terminal B becomes visible
- **AND** plan panel shows session B's plan content
- **AND** tasks panel shows session B's tasks
- **AND** activity log shows session B's events
- **AND** status bar shows session B's mode and cost

### Requirement: Resume session
The system SHALL resume a previously created session by creating a new PTY and executing `claude --resume <session_id>` in the worktree directory.

#### Scenario: Resume idle session
- **WHEN** user clicks on a session with status `idle`
- **THEN** system creates a PTY with cwd = session's worktree path
- **AND** writes `claude --resume <claude_session_id>` to the PTY
- **AND** session status changes to `running`

### Requirement: Delete session
The system SHALL delete a session by killing its PTY, removing the git worktree, and cleaning up state.

#### Scenario: Delete session with worktree
- **WHEN** user deletes a session that has an associated worktree
- **THEN** system kills the PTY if running
- **AND** executes `git worktree remove <path>`
- **AND** removes the session from workspace state

### Requirement: Session status tracking
Each session SHALL have a status: `idle`, `running`, `needs_attention`, `completed`. Status SHALL update based on hook events from Claude CLI.

#### Scenario: Session starts running
- **WHEN** hook server receives `SessionStart` event with matching session_id
- **THEN** session status changes to `running`

#### Scenario: Agent needs attention
- **WHEN** hook server receives `Stop` event (Claude waiting for input)
- **THEN** session status changes to `needs_attention`
- **AND** sidebar shows visual indicator (orange dot) on the session
