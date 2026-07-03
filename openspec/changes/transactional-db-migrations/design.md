## Context

The runner (`db.rs:273-282`) applies DDL and bumps `schema_version` as two separate statements with no transaction. A crash between them — or between two statements inside one migration's batch — leaves a half-applied, non-re-runnable schema that fails `Database::open()` forever. Migration 022 already worked around this locally with an inner `BEGIN…COMMIT` and a comment explicitly warning not to add a runner-level transaction without removing the inner one.

## Goals

- Make every migration atomic (DDL + version bump commit together or not at all).
- Do it once, at the runner, so no migration author has to remember it (the 022 workaround is exactly the per-file burden we want to eliminate).
- Do not break migration 022's nested-transaction constraint.

## Decision 1: Transaction at the runner, remove per-file transactions

**Chosen**: the runner wraps each migration; migration 022's inner `BEGIN…COMMIT` is deleted.

- **Alternative A**: leave 022 as-is and special-case it in the runner (skip wrapping when the SQL contains `BEGIN`) — rejected: fragile string-sniffing, and it perpetuates the per-file burden for future authors.
- **Alternative B**: require every migration to self-wrap (formalize 022's pattern) — rejected: relies on author discipline, which is what failed for 29 migrations; the runner is the right single owner.
- **Trade-off**: touches an existing migration file (022). Acceptable — it is a comment+`BEGIN`/`COMMIT` deletion, and the runner's transaction produces identical net effect.

## Decision 2: `unchecked_transaction()` vs `&mut self` transaction

`migrate()` currently borrows `&self`. `rusqlite`'s `Connection::transaction()` needs `&mut self`; `unchecked_transaction()` works on `&self` (the caller asserts no overlapping transaction, true here — migration runs once at open, single-threaded).

**Chosen**: `unchecked_transaction()` to avoid churning `migrate()`'s signature and its callers. If implementation reveals `&mut self` is already available or cleaner, use `transaction()` instead — either satisfies the spec.

## Decision 3: Version-check stays outside the transaction

The `if version > current` gate and `tracing::info!` remain outside the transaction. Only the `execute_batch(sql)` + `schema_version` insert become the atomic unit. This keeps the loop structure and logging unchanged.

## Risks

- **Migration 022 interaction** (the one real subtlety): after deleting its inner `BEGIN/COMMIT`, its multi-statement rebuild must run correctly inside the runner's transaction. Mitigation: covered by the fresh-DB migration test (asserts final `schema_version` and that 022's tables/FK exist).
- **A migration that genuinely needs statements outside a transaction** (e.g. `PRAGMA foreign_keys` toggles, `VACUUM`) — none exist in the current set; if a future migration needs it, that is a per-migration escape hatch to design then, not now. Note it in the runner comment.
- **DDL-in-transaction support**: SQLite supports transactional DDL (`ALTER TABLE`, `CREATE TABLE`), so wrapping is sound. `VACUUM`/some `PRAGMA` cannot run in a transaction — confirmed absent from the current migration set at implementation time.

## Migration / rollout

No data migration; this changes *how* migrations apply, not the schema. Ship unconditionally — strictly safer. Existing users already past a given migration are unaffected (the `version > current` gate skips applied ones).
