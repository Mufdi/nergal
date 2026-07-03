## Context

`sessions` children split into two generations: migration-022-era tables (`session_transcripts`, `session_summaries`) carry `REFERENCES sessions(id) ON DELETE CASCADE`; the 001-era tables (`tasks`, `cost_summaries`) don't. `PRAGMA foreign_keys=ON` is set on every connection open (`db.rs:199,216`) and in test helpers (`db.rs:1755,1892`), so declared FKs are actually enforced. `delete_session` (`db.rs:1013-1017`) is a single `DELETE FROM sessions`.

## Goals / Non-Goals

**Goals:**
- A deleted session leaves zero orphan `tasks`/`cost_summaries` rows, enforced by the schema (not by remembering to delete).
- Purge orphans that already exist.

**Non-Goals:**
- A general FK audit of non-session tables; a down-migration framework; touching the ClickUp/Linear mirror tables (different lifecycle — tracker-owned, tombstoned).

## Decisions

### D1: table-rebuild-with-FK over explicit deletes in `delete_session`

Chosen: rebuild `tasks` and `cost_summaries` with the cascade FK (SQLite cannot `ALTER TABLE … ADD CONSTRAINT`; rebuild is the only way, and 022 already proved the pattern in this codebase).

**Alternative — explicit `delete_tasks_for_session` / `delete_costs_for_session` calls in `delete_session`:**
- \+ no data movement, trivially safe migration.
- − fixes the symptom at one call site: any future deletion path (bulk cleanup, workspace cascade — note `workspaces → sessions` already cascades via 001:10, so deleting a workspace today orphans grandchildren without ever calling `delete_session`!) re-introduces the leak. That workspace-cascade hole is only closable schema-side.
- − leaves the existing orphans in place (would need a separate sweep anyway).

The workspace-cascade argument is decisive: `DELETE FROM workspaces` cascades to `sessions` at the SQL layer, bypassing any Rust helper. Only a schema-level FK covers all paths. (Root-cause over patch, per project rules.)

### D2: orphan sweep via `INSERT … SELECT … JOIN sessions`

The rebuild copies only rows whose `session_id` still exists (`INSERT INTO tasks_new SELECT t.* FROM tasks t JOIN sessions s ON s.id = t.session_id`). Orphans are unreachable by every read path (all queries key by a live session id), so dropping them is safe and makes the FK addition valid (a rebuild that carried orphans over would violate the new FK at copy time).

### D3: transaction ownership follows `transactional-db-migrations`

- If `transactional-db-migrations` (pending Band-A change) lands first: no inner `BEGIN/COMMIT`; the runner owns atomicity.
- If this lands first: wrap the whole migration in `BEGIN…COMMIT` + lead with `DROP TABLE IF EXISTS tasks_new; DROP TABLE IF EXISTS cost_summaries_new;` exactly like 022 (`022:31-46`), including the workaround comment so the other change knows to strip it.
- Implementation MUST check which state holds at build time and follow the matching branch.

### D4: preserve exact column definitions and indexes

`tasks_new`/`cost_summaries_new` restate the current effective schema of each table (001 plus any later `ALTER TABLE … ADD COLUMN` — verify against the full migration set at implementation time, not just 001), changing only the `session_id` column to add the FK clause. Recreate `idx_tasks_session` after the rename.

## Risks / Trade-offs

- [Crash mid-rebuild leaves `*_new` + original] → `DROP TABLE IF EXISTS *_new` head statements make re-run clean (022 precedent); transactional runner (D3) removes the window entirely.
- [Restated schema drifts from effective schema (a later migration added a column this rebuild misses)] → implementation task explicitly diffs `PRAGMA table_info(tasks)` / `(cost_summaries)` on a fully-migrated dev DB against the rebuild DDL before shipping.
- [Large `tasks` table makes the copy slow on old profiles] → single sequential scan + insert, no per-row work; acceptable for a startup migration (022 set the precedent with `session_summaries`).
- [`foreign_keys` pragma off on some future connection silently disables enforcement] → out of scope here, but the new capability spec pins the enforced behavior, making a regression spec-visible.

## Open Questions

- None blocking. Whether other 001-era tables (`annotations` — rebuilt in 004 with FK, verified) need the same treatment was checked: `tasks` and `cost_summaries` are the only remaining `session_id` tables without the FK.
