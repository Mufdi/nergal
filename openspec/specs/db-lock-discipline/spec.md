# db-lock-discipline Specification

## Purpose
TBD - created by archiving change release-db-lock-during-io. Update Purpose after archive.
## Requirements
### Requirement: Guard-free slow work in session/git commands

`delete_session`, `create_pr`, and `git_ship` SHALL extract all needed DB state under a short-lived guard, release it before performing git/network work (worktree removal, MOC git diff, push, `gh pr create`, ship pipeline), and re-acquire only for final writes. A finalize-phase write SHALL re-validate row existence and no-op gracefully if the row is gone.

#### Scenario: ship does not freeze the data layer

- **GIVEN** `git_ship` is pushing to a slow remote
- **WHEN** another command calls `db.lock()` during the push
- **THEN** it acquires the guard without waiting for the push to finish

#### Scenario: delete_session with slow worktree removal

- **GIVEN** a session delete whose worktree removal takes seconds
- **WHEN** the sessions list refreshes concurrently
- **THEN** the refresh completes without blocking on the removal

#### Scenario: row deleted between phases

- **GIVEN** `delete_session` released the guard for its work phase
- **WHEN** the session row was already removed by the time it re-acquires
- **THEN** the command completes Ok without error

