## Why

Deleting a session leaves its `tasks` and `cost_summaries` rows behind forever. `src-tauri/migrations/001_initial.sql` declares `tasks.session_id TEXT NOT NULL` (line 23) and `cost_summaries.session_id TEXT PRIMARY KEY` (line 36) with **no** `REFERENCES sessions(id) ON DELETE CASCADE` — only an index (`idx_tasks_session`, line 33). `Database::delete_session` (`src-tauri/src/db.rs:1013-1017`) runs only `DELETE FROM sessions`. By contrast, `session_transcripts`/`session_summaries` got exactly this FK in migration 022 (`022_session_transcripts.sql:22,34` — "so a deleted session leaves no orphan row"), and `sessions.workspace_id` itself cascades from `workspaces` (001:10). `PRAGMA foreign_keys=ON` is already set on every connection (`db.rs:199,216`), so the missing FKs are the only gap. Orphan rows accumulate unboundedly in a long-lived profile that spawns many worktree sessions.

## What Changes

- **New migration `src-tauri/migrations/032_session_child_fk_cascade.sql`** rebuilding `tasks` and `cost_summaries` with `session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE` — the same table-rebuild pattern migration 022 used for `session_summaries` (create `_new` table with FK → `INSERT INTO … SELECT` → drop old → rename → recreate indexes).
- **One-time orphan sweep inside the same migration**: the rebuild's `INSERT … SELECT` joins against `sessions` so pre-existing orphan rows are dropped rather than carried over (they reference sessions that no longer exist and are unreachable by any read path).
- **No change to `delete_session`** — with the FKs in place and `foreign_keys=ON` already enforced, the single `DELETE FROM sessions` is the complete, correct implementation (this is the root-cause fix; per-table explicit deletes were considered and rejected, see design.md).
- **Sequencing note**: pairs with the pending `transactional-db-migrations` change (a runner-level transaction makes a crash mid-rebuild safe). If this lands first, the migration wraps its rebuild in its own `BEGIN…COMMIT` exactly like 022 does today; if it lands after, no inner transaction (the runner owns it — 022's comment at `022:13-15` documents the nesting hazard).

## Capabilities

### New Capabilities

- `session-data-lifecycle`: the contract that session-scoped DB rows (tasks, cost summaries, transcripts, summaries) share their session's lifetime — deleting a session removes them, and no orphan rows survive.

### Modified Capabilities

_None._ (The audit handoff suggested MODIFIED `session-directory`, but on inspection that spec covers the MCP session-directory tool surface (`list_sessions`/`get_session`), not DB row lifecycle — no existing capability fits, so this is a new one per the "New only when none fits" rule.)

## Impact

- **`src-tauri/migrations/032_session_child_fk_cascade.sql`** (new): rebuild of `tasks` + `cost_summaries` with the cascade FK; recreate `idx_tasks_session`. 032 is the next free number after 031 (claimed by the pending `clickup-subdata-indexes` change) — re-verify at implementation time.
- **`src-tauri/src/db.rs`**: register the migration in the runner's list; no method changes.
- **Tests**: extend the existing DB tests — create session + task + cost row, `delete_session`, assert child rows are gone; and assert a pre-seeded orphan row does not survive the migration.
- **Risk**: MEDIUM — table rebuild moves data; mitigations are the 022-proven pattern, the idempotence-guard `DROP TABLE IF EXISTS *_new` head statement (022:31 precedent), and the transactional-runner pairing.
- **Out of scope**: FKs for tables that already have them (021/022/028 children); any UI change.
