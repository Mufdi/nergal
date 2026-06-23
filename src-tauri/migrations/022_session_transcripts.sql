-- Pull-based lazy summaries (nergal-mcp-server Revision 1).
--
-- Two changes:
--   1. session_transcripts — durable per-session marker (transcript path +
--      last_stop_at) written cheaply on every Stop, so summary generation can
--      be pulled lazily on read (and a recently-dead session can still be
--      summarized from its on-disk transcript after a restart).
--   2. Rebuild session_summaries to add the FK that 021 shipped without, so a
--      deleted session leaves no orphan row.
--
-- 022 manages its own transaction. The migration runner (db.rs::migrate) calls
-- execute_batch with NO enclosing transaction, and SQLite does not auto-rollback
-- a mid-batch statement error — so the rebuild is wrapped in BEGIN…COMMIT here.
-- Do NOT change the runner to wrap migrations in its own transaction without
-- removing this BEGIN (nested transactions are an error in SQLite).
--
-- foreign_keys=ON is already set on every connection (db.rs:135/152/1212). No
-- PRAGMA foreign_keys=OFF dance is needed: nothing references session_summaries,
-- so renaming it under enforcement is safe.

CREATE TABLE IF NOT EXISTS session_transcripts (
    session_id     TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    transcript_path TEXT NOT NULL,
    last_stop_at   INTEGER NOT NULL
);

-- Rebuild session_summaries with the missing FK. DROP IF EXISTS at the head
-- recovers from a partial-apply on a prior boot; the orphan filter in the copy
-- prevents a FOREIGN KEY constraint failure on rows the 021 leak already
-- orphaned (those rows are regenerable, so dropping them is harmless).
DROP TABLE IF EXISTS session_summaries_new;
BEGIN;
CREATE TABLE session_summaries_new (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    summary    TEXT NOT NULL,
    model      TEXT,
    token_cost INTEGER,
    updated_at INTEGER NOT NULL
);
INSERT INTO session_summaries_new (session_id, summary, model, token_cost, updated_at)
    SELECT session_id, summary, model, token_cost, updated_at
    FROM session_summaries
    WHERE session_id IN (SELECT id FROM sessions);
DROP TABLE session_summaries;
ALTER TABLE session_summaries_new RENAME TO session_summaries;
COMMIT;
