# db-lock-discipline Specification

## Purpose
TBD - created by archiving change release-db-lock-during-io. Update Purpose after archive.
## Requirements
### Requirement: Guard-free slow work in session/git commands

No code path SHALL hold the global `Mutex<Database>` guard across git or `gh` subprocess execution, network I/O, or filesystem I/O. Any command, poller, hook handler, MCP tool, or startup/shutdown sweep that needs slow work SHALL extract all required DB state under a short-lived guard, release it (scope block or explicit `drop`) before performing the slow work, and — where a write follows — re-acquire the guard only for the final write. A finalize-phase write SHALL re-validate row existence and no-op gracefully if the row is gone. Per-N loops that mix DB reads with filesystem I/O SHALL gather every iteration's DB-derived inputs under one short guard, release it, then perform the filesystem I/O guard-free.

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

#### Scenario: git push wrapper does not hold the guard

- **GIVEN** any `worktree::*` command wrapper (`git_push`, `git_commit`, `poll_pr_checks`, `pull_target_into_session`, …) that reads a cwd/branch from the DB
- **WHEN** it invokes the git/`gh` subprocess
- **THEN** the DB guard has already been dropped, so a concurrent `db.lock()` caller is not blocked by the subprocess

#### Scenario: per-session close sweep does not serialize on file I/O

- **GIVEN** `delete_workspace` (or `queue_close_markers`, or the `lib.rs` crash-recovery drain) iterating over N sessions to write MOC / session-log files
- **WHEN** it performs the filesystem writes for each session
- **THEN** the DB guard is not held across the loop's filesystem I/O — inputs were gathered under one short guard first

### Requirement: DB-plus-I/O helpers expose a guard-free write phase

Helper functions that both read the database and write to the filesystem (notably `MocBuilder::build`) SHALL separate their DB-read phase from their filesystem-write phase, so callers can perform the read under the guard and the write with the guard released. The DB-read phase MAY borrow `&Database`; the write phase MUST NOT.

#### Scenario: MOC render/write runs guard-free

- **GIVEN** a caller building a session MOC while holding the DB guard for the read phase
- **WHEN** it renders and writes the markdown file to the vault
- **THEN** the write executes after the guard is released, borrowing no `&Database`

