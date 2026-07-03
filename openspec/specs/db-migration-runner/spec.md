# db-migration-runner Specification

## Purpose
TBD - created by archiving change transactional-db-migrations. Update Purpose after archive.
## Requirements
### Requirement: Atomic migration application

The migration runner SHALL apply each migration's DDL and its `schema_version` bump within a single transaction, committing both together or neither. An interruption (process kill, error) during a migration SHALL leave that migration entirely unapplied, so the next startup re-runs it cleanly.

Migration SQL files SHALL NOT contain their own `BEGIN`/`COMMIT` (the runner owns the transaction; a nested transaction is a SQLite error).

#### Scenario: multi-statement migration is atomic under interruption

- **GIVEN** a migration that adds two columns in one batch
- **WHEN** the process is killed after the first `ALTER TABLE` but before the second
- **THEN** on restart the migration re-runs from the top, the first column is not already present, and the migration completes successfully without a "duplicate column" error

#### Scenario: version bump is tied to the DDL

- **WHEN** a migration's DDL commits
- **THEN** its `schema_version` row is committed in the same transaction, so the runner never sees applied DDL with an unbumped version (or vice versa)

#### Scenario: clean fresh-DB migration

- **WHEN** the full migration set runs on a fresh database
- **THEN** every migration applies in order and the final `schema_version` equals the highest migration number

