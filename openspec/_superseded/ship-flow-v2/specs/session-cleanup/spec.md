## ADDED Requirements

### Requirement: Total session deletion on terminal state
When a session reaches a terminal state (PR confirmed merged on remote OR explicit user "Cleanup session" action after a successful local merge), the system SHALL delete every persisted artifact tied to that session: the SQLite session row, its worktree directory on disk, its git branch, its plan files (`.claude/plans/<session_id>/`), and any transcript files associated with the session. The previous `update_session_status("completed")` soft-archive behavior SHALL be replaced.

#### Scenario: PR-merged poll triggers full cleanup
- **WHEN** the PR-status poller observes that the session's PR has transitioned from `OPEN` to `MERGED` on remote
- **THEN** the system deletes the DB row for the session, removes its worktree via `git worktree remove`, deletes the branch via `git branch -D`, removes the session's plan-files directory, and removes any transcript files keyed by the session id

#### Scenario: User confirms cleanup after local merge
- **WHEN** the user invokes the "Cleanup session" action after a successful local squash merge
- **THEN** the system performs the same total deletion as the PR-merged path

#### Scenario: Cleanup is the only path that ends a session
- **WHEN** any code path that previously called `update_session_status("completed")` is reached
- **THEN** that call SHALL be replaced by the total-deletion path (no session row remains in `completed` state going forward)

#### Scenario: Toast confirms deletion with create-new CTA
- **WHEN** total deletion completes successfully
- **THEN** a toast appears with text "Session closed" and a CTA "Create new (Ctrl+N)" that opens a new session in the same workspace when activated

### Requirement: Cleanup is idempotent and partial-failure tolerant
The system SHALL treat each artifact deletion (DB row, worktree, branch, plan files, transcript) as independently best-effort: failure of one artifact SHALL NOT block deletion of the others. The system SHALL log each failure but proceed with remaining deletions.

#### Scenario: Worktree directory is missing
- **WHEN** cleanup runs and `git worktree remove` fails because the directory was already deleted manually
- **THEN** the system logs the warning, continues to delete the branch, plan files, transcript, and DB row, and reports overall success

#### Scenario: Branch deletion fails because branch is checked out elsewhere
- **WHEN** `git branch -D` fails because the branch is in use
- **THEN** the system logs the warning, continues with the rest of the cleanup, and surfaces a non-blocking toast indicating the branch was retained

### Requirement: Cleanup never runs silently
The system SHALL NOT trigger total deletion as a silent side effect of any unrelated operation. Cleanup SHALL only run from (a) the PR-merged poll's confirmed-merged branch, or (b) an explicit user action originating from the GitPanel.

#### Scenario: Failed Ship does not trigger cleanup
- **WHEN** Ship fails at the push or PR creation stage
- **THEN** no cleanup is performed; the session and worktree remain intact

#### Scenario: User dismissing a dialog does not trigger cleanup
- **WHEN** the user closes the ShipDialog or MergeModal without completing the action
- **THEN** no cleanup is performed
