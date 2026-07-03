# session-data-lifecycle Specification

## Purpose
TBD - created by archiving change session-child-fk-cascade. Update Purpose after archive.
## Requirements
### Requirement: Session children cascade on delete

Every session-lifetime table keyed by `session_id` (`tasks`, `cost_summaries`, `session_transcripts`, `session_summaries`, `annotations`) SHALL declare `REFERENCES sessions(id) ON DELETE CASCADE`, so deleting a session row removes all its child rows at the SQL layer regardless of which code path performed the delete. Deliberate exception: the cross-session message store (`cross_session_threads`/`cross_session_messages`) carries no session FK by design — it is a durable audit log that must survive session deletion (`028_cross_session.sql:16-18`).

#### Scenario: delete_session removes children

- **GIVEN** a session with rows in `tasks` and `cost_summaries`
- **WHEN** `delete_session` runs
- **THEN** the session's `tasks` and `cost_summaries` rows are gone

#### Scenario: workspace cascade reaches grandchildren

- **GIVEN** a workspace with a session that has `tasks` rows
- **WHEN** the workspace row is deleted (cascades to `sessions` via the 001 FK)
- **THEN** the session's `tasks` rows are also removed — no Rust helper involved

### Requirement: Migration purges existing orphans

The migration adding the FKs SHALL drop pre-existing orphan rows (child rows whose `session_id` no longer exists in `sessions`) as part of the table rebuild, and SHALL be cleanly re-runnable after an interruption.

#### Scenario: orphans do not survive the rebuild

- **GIVEN** a database with `tasks` rows referencing a deleted session
- **WHEN** the migration runs
- **THEN** the rebuilt `tasks` table contains only rows whose session exists

#### Scenario: interrupted rebuild re-runs cleanly

- **WHEN** the process dies mid-migration and the app restarts
- **THEN** the migration re-runs without "table already exists" or FK errors

