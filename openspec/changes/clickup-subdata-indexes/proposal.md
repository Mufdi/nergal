## Why

The ClickUp mirror's per-task subdata tables have no index on `task_id`, so the panel's only read path runs two correlated full-table scans **per task row** on every panel mount and every 45s poll refresh. `src-tauri/migrations/015_clickup_mirror.sql` defines `clickup_checklists` (line 79), `clickup_comments` (94), and `clickup_attachments` (104) with only `PRIMARY KEY (id)` — the file's only indexes (59-61) are on `clickup_tasks`. Tombstoned rows (7-day retention) keep the scanned tables growing.

## What Changes

- **New migration `src-tauri/migrations/031_clickup_subdata_indexes.sql`** adding:
  - `CREATE INDEX IF NOT EXISTS idx_clickup_checklists_task ON clickup_checklists(task_id);`
  - `CREATE INDEX IF NOT EXISTS idx_clickup_attachments_task ON clickup_attachments(task_id);`
  - `CREATE INDEX IF NOT EXISTS idx_clickup_comments_task ON clickup_comments(task_id);`
- The first two back the correlated `SELECT COUNT(*)` subqueries in `read_tasks` (`src-tauri/src/clickup/mirror.rs:398-399` — "the panel's only read path", called on every panel mount and on each `clickup:changed` emission from the 45s poller: `poller.rs:26`, `emit_changed` ~`poller.rs:930`, store listener `src/stores/clickup.ts:692`). The comments index backs the detail view's `FROM clickup_comments WHERE task_id = ?1` (`mirror.rs:632`).
- No code changes — SQLite picks the indexes up automatically.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `clickup-mirror`: subdata reads (checklist/attachment counts per task, comments per task) are index-backed rather than full scans.

## Impact

- **`src-tauri/migrations/031_clickup_subdata_indexes.sql`** (new): three `CREATE INDEX IF NOT EXISTS` statements. 031 is the next free number today (030 is the highest on disk and no pending change claims 031) — re-verify at implementation time.
- **Write-side cost**: negligible — the mirror reconcile inserts/deletes subdata rows in batches; three small indexes do not change its complexity class.
- **Risk**: LOW — additive DDL, idempotent (`IF NOT EXISTS`), no schema rebuild, no data movement.
- **Out of scope**: query-shape changes in `read_tasks` (e.g. JOIN+GROUP BY instead of correlated subqueries); index coverage for the Linear mirror (separate tables, not audited as hot).
