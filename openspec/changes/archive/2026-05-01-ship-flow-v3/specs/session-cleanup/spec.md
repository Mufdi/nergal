## ADDED Requirements

### Requirement: Cleanup runs only on explicit user trigger
Total session cleanup (worktree + branch + DB row deletion, preceded by plan archive) SHALL be invoked exclusively from one of two paths:
1. The PR Viewer's "Merge into `<base>`" success handler when `gh pr merge` returns success.
2. An explicit "Cleanup session" button rendered in the GitPanel header when the session's PR is in a non-OPEN state (MERGED or CLOSED) — kept as a recovery path for sessions that were merged outside cluihud.

There SHALL be no background poll that auto-invokes cleanup.

#### Scenario: Merge success triggers cleanup
- **WHEN** the user clicks "Merge into main" in the PR Viewer and `gh pr merge` returns success
- **THEN** the system synchronously invokes the cleanup sequence

#### Scenario: PR merged externally surfaces recovery banner
- **WHEN** the GitPanel polls PR status and observes the PR is MERGED or CLOSED
- **THEN** the GitPanel header renders a banner "PR is `<state>` — Cleanup session" with an explicit button; cleanup runs only if the user clicks that button

#### Scenario: No silent cleanup paths
- **WHEN** any other code path completes (Ship, Push, commit, anything else)
- **THEN** cleanup is NOT invoked; the user must take an explicit action to delete the session

### Requirement: Cleanup sequence
When invoked, cleanup SHALL execute in this order, treating each step as best-effort and continuing on individual failure with a warning:

1. Archive plans (per `plan-archive-on-cleanup`).
2. Remove the worktree directory via `git worktree remove`.
3. Delete the branch via `git branch -D`.
4. Delete the session's row from the DB.
5. Remove the session id from `sessionTabIdsAtom`; if it was the active session, clear `activeSessionIdAtom`.

After completing, the system SHALL emit an aggregated `CleanupResult { deleted: bool, warnings: Vec<String>, archived_plans_path: Option<String> }` for the frontend to surface.

#### Scenario: Worktree directory missing
- **WHEN** cleanup runs and `git worktree remove` fails because the directory was already deleted manually
- **THEN** the system logs a warning, includes it in `warnings`, and continues with the remaining steps

#### Scenario: Toast confirms outcome with archive path
- **WHEN** cleanup completes successfully with plans archived to `<path>`
- **THEN** the toast shows "Session merged and archived. Plans saved to `<path>`."

### Requirement: Post-cleanup transition
After cleanup deletes the session, the system SHALL transition the UI as follows:

- Find the workspace's session with the most recent `updated_at` that is not the deleted session.
- If a candidate exists: switch `activeSessionIdAtom` to that session, open or focus its tab in the right panel, show a brief toast "Switched to session: `<name>`".
- If no candidate exists: clear `activeSessionIdAtom`, close the right panel (set `rightPanelOpenAtom` to false), and the workspace shows its empty state until the user creates or selects a session.

#### Scenario: Switches to most recent remaining session
- **WHEN** cleanup completes and the workspace has 2 other sessions (one updated 1h ago, one 5m ago)
- **THEN** `activeSessionIdAtom` becomes the 5-minute-old session and a toast confirms the transition

#### Scenario: Cold start when no sessions remain
- **WHEN** cleanup completes and the workspace has no other sessions
- **THEN** the right panel closes and the workspace renders its empty state (matching the cold-start UI)
