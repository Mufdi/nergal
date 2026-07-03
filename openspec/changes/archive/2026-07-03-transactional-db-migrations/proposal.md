## Why

The SQLite migration runner applies each migration **without a transaction**, so a crash mid-migration can permanently brick app startup.

`migrate()` (`db.rs:273-282`) runs each migration file via `self.conn.execute_batch(sql)?` and then, as a **separate** statement, inserts the new `schema_version`. Several migrations contain multiple non-idempotent `ALTER TABLE ... ADD COLUMN` statements in one batch — e.g. `018_clickup_session_binding.sql` adds two columns; `023_linear_mirror.sql` is larger still. SQLite does not auto-rollback a mid-batch statement error.

Failure mode: a process kill (OOM, power loss, user quit) between two `ALTER TABLE` statements in one migration leaves the columns partially added and `schema_version` **unbumped**. On next launch the same migration re-runs from the top; the already-added `ADD COLUMN` fails with "duplicate column name"; `?` propagates out of `migrate()`; `Database::open()` fails; the app cannot start until the SQLite file is hand-edited. The team already knows this — migration `022_session_transcripts.sql:13-15` documents the gap verbatim and works around it with its own inner `BEGIN…COMMIT`, but the runner was never fixed for the other 29 migrations.

## What Changes

- **Wrap each migration in one transaction at the runner level.** In `migrate()`, per migration whose `version > current`, open an `unchecked_transaction()`, run `execute_batch(sql)`, insert the `schema_version` row, and `commit()` — so the schema change and its version bump are atomic. A crash anywhere inside leaves the migration entirely unapplied and re-runnable cleanly.
- **Remove migration 022's now-redundant inner `BEGIN…COMMIT`.** Its own comment (`022:13-15`) warns that a runner-level transaction plus its inner `BEGIN` would nest, and SQLite errors on nested transactions. This change makes the runner own the transaction, so 022's `BEGIN;`/`COMMIT;` lines (022:32,46) are deleted; its rebuild statements ride the runner's transaction like every other migration.
- **Keep the version check outside/around the transaction** — the `if version > current` gate and the per-migration `tracing::info!` are unchanged; only the apply+bump pair becomes atomic.

## Capabilities

### New Capabilities

- `db-migration-runner` — the contract that schema migrations apply atomically: each migration's DDL and its `schema_version` bump either both commit or neither does, so an interrupted migration is always cleanly re-runnable and never leaves a half-applied schema that fails on restart.

## Impact

- **`src-tauri/src/db.rs`**: `migrate()` (273-282) wraps each migration in `unchecked_transaction()` + `commit()`.
- **`src-tauri/migrations/022_session_transcripts.sql`**: delete the inner `BEGIN;` (line 32) and `COMMIT;` (line 46) and the two comment lines describing the workaround (13-15) — the runner now guarantees atomicity.
- **`rusqlite` API note**: `unchecked_transaction()` is used (not `transaction(&mut self)`) because `migrate()` holds `&self`, not `&mut self`; verify the borrow shape at implementation time and adjust the signature if a `&mut` transaction is cleaner.
- **Tests**: a test that runs the migration set on a fresh DB and asserts final `schema_version`; a fault-injection test that aborts a multi-statement migration mid-way (e.g. a migration whose second statement is forced to error) and asserts the first statement did **not** persist and `schema_version` is unchanged, so a re-run succeeds.
- **Risk**: MED-HIGH failure mode (total app unavailability) but LOW change complexity; the one subtlety is the 022 nested-transaction interaction, explicitly handled.
- **Out of scope**: rewriting existing migrations to be idempotent (unnecessary once atomic); a down-migration / rollback framework (not needed for the failure mode addressed).
