## 1. Migration

- [x] 1.1 Create `src-tauri/migrations/031_clickup_subdata_indexes.sql` with `CREATE INDEX IF NOT EXISTS idx_clickup_checklists_task ON clickup_checklists(task_id);`, `CREATE INDEX IF NOT EXISTS idx_clickup_attachments_task ON clickup_attachments(task_id);`, `CREATE INDEX IF NOT EXISTS idx_clickup_comments_task ON clickup_comments(task_id);`. Re-verify 031 is still the next free number in `src-tauri/migrations/` and register the file the same way existing migrations are registered in `db.rs`'s migration list.

## 2. Sanity check

- [ ] 2.1 On a dev DB with mirror data, run `EXPLAIN QUERY PLAN` for the `read_tasks` statement (`src-tauri/src/clickup/mirror.rs:391-405`) and confirm the checklist/attachment subqueries hit the new indexes.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Manual: launch the app with an existing ClickUp-enabled profile → migration applies cleanly; panel loads; second launch is a no-op (idempotent).
  - 1.1 NOTE: implemented as migration 032 (renumbered from 031 — session-child-fk-cascade took 031; runner versions are positional). 2.1 (EXPLAIN QUERY PLAN) + 3.2 (live profile walk) pending: sqlite3 CLI absent on host; index usage is deterministic for the equality/correlated-count read paths.
