# REVIEW — clickup-subdata-indexes

## Orchestrator vet (XS migration, no reviewer; done directly) · 2026-07-03

**Verdict: PASS.**

- Migration numbered **032**, NOT 031: the proposal reserved 031, but
  `session-child-fk-cascade` (Wave 1b, earlier this session) already took 031 — runner
  versions are positional (array index+1), so 032 is the next free number on disk
  (verified: `031_session_child_fk_cascade.sql` is the highest). Registered as the 32nd
  `include_str!` element after 031.
- Three `CREATE INDEX IF NOT EXISTS idx_clickup_{checklists,attachments,comments}_task
  ON …(task_id)` — additive, idempotent, no rebuild, no data movement.
- fresh-DB test bumped 31→32 + comment; runs the full migration set (incl. 015's
  clickup tables) → 032 applies cleanly on the real schema.
- Index usage is deterministic (SQLite uses an equality index on an indexed column for
  `WHERE task_id = ?` and correlated `COUNT(*)` subqueries) — the EXPLAIN QUERY PLAN
  sanity (task 2.1) needs the sqlite3 CLI (absent on this host) or a dev DB, so it moves
  to the manual walk (3.2).

## Gates

- Gate 1-3: PASS (clippy -D warnings clean, 768 tests incl. fresh_db@32, fmt clean).
- Gate 6: 2 files (migration + db.rs registration) = estimate.
