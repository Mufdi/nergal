## 1. Runner-level transaction

- [ ] 1.1 `migrate()` (`db.rs:273-282`) — per migration with `version > current`: open `self.conn.unchecked_transaction()`, run `execute_batch(sql)`, insert the `schema_version` row, `commit()`. Keep the `version > current` gate and `tracing::info!` outside the transaction. (Switch to `transaction(&mut self)` only if the borrow shape is already `&mut`.)
- [ ] 1.2 Add a runner comment: migrations must not contain their own `BEGIN/COMMIT`; statements that cannot run in a transaction (`VACUUM`, some `PRAGMA`) are unsupported by this runner (none exist today).

## 2. Remove migration 022's inner transaction

- [ ] 2.1 `022_session_transcripts.sql` — delete `BEGIN;` (line 32) and `COMMIT;` (line 46); delete the workaround comment (the ~5 lines describing the no-transaction gap, roughly lines 11-15 — trim to the actual comment block, not a fixed range). **Keep** the `DROP TABLE IF EXISTS session_summaries_new` (022:31): it is deliberate partial-apply recovery for users who already have a half-applied rebuild from an older binary. Keep it at the head of the file — it runs as the first statement *inside* the runner's transaction; on rollback the leftover table is restored and re-cleaned on the next run, so the recovery semantics are preserved. The rebuild statements ride the runner's transaction.

## 3. Tests

- [ ] 3.1 Fresh-DB test: run the full migration set, assert final `schema_version` equals the highest migration number and that 022's `session_transcripts` table + `session_summaries` FK exist.
- [ ] 3.2 Fault-injection test: **extract a testable seam first** — the migration list is a hardcoded `include_str!` array inside `migrate()`; refactor to an `apply_migration(conn, version, sql)` helper the runner loops over, so a synthetic multi-statement migration can be injected. Then: apply one whose second statement errors; assert the first statement did not persist and `schema_version` is unchanged; re-run and assert it now succeeds.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.
- [ ] 4.2 Manual: delete a dev DB, launch the app, confirm it boots and reaches the current schema; confirm no "duplicate column" on a second launch.
