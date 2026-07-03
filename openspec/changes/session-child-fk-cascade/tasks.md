## 1. Migration

- [ ] 1.1 Diff the effective schemas first: on a fully-migrated dev DB, capture `PRAGMA table_info(tasks)` and `PRAGMA table_info(cost_summaries)` and write the rebuild DDL from that (001 base + any later ADD COLUMNs), changing only `session_id` to add `REFERENCES sessions(id) ON DELETE CASCADE`.
- [ ] 1.2 Create `src-tauri/migrations/032_session_child_fk_cascade.sql`: `DROP TABLE IF EXISTS tasks_new; DROP TABLE IF EXISTS cost_summaries_new;` head guards; create `*_new` with FK; `INSERT INTO tasks_new SELECT t.* FROM tasks t JOIN sessions s ON s.id = t.session_id` (same shape for `cost_summaries`); drop originals; rename; recreate `idx_tasks_session`. Re-verify 032 is still the next free number (031 is claimed by pending `clickup-subdata-indexes`).
- [ ] 1.3 Transaction ownership per design D3: if `transactional-db-migrations` has landed, no inner `BEGIN/COMMIT`; otherwise wrap in `BEGIN…COMMIT` with the 022-style comment.
- [ ] 1.4 Register the migration in `db.rs`'s migration list.

## 2. Tests

- [ ] 2.1 DB test: create workspace + session + `tasks` row + `cost_summaries` row; `delete_session`; assert both child rows gone.
- [ ] 2.2 DB test: delete the workspace row directly; assert the session AND its `tasks`/`cost_summaries` rows are gone (grandchild cascade).
- [ ] 2.3 Migration test: seed a pre-032 shaped DB with an orphan `tasks` row; run migrations; assert the orphan is gone and non-orphan rows survived with identical column values.

## 3. Verification

- [ ] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Manual: launch with an existing dev profile → migration applies; sessions panel works; delete a session; inspect the DB shows no rows for its id in `tasks`/`cost_summaries`.
