-- tasks and cost_summaries shipped in 001 without a FK to sessions (only an
-- index), unlike session_transcripts/session_summaries which got the FK in
-- 022. delete_session (db.rs) runs a single DELETE FROM sessions and relies
-- on cascading FKs to clean up children; without this FK, tasks and
-- cost_summaries rows outlive their session forever, and deleting a
-- workspace (which cascades to sessions via 001) orphans them too since no
-- Rust helper is ever called on that path.
--
-- The runner now owns the migration transaction (transactional-db-migrations),
-- so this file must not contain its own BEGIN/COMMIT. The DROP TABLE IF
-- EXISTS head guards remain for partial-apply recovery (022 precedent).
--
-- The INSERT ... SELECT ... JOIN sessions filters out pre-existing orphans
-- (rows whose session_id no longer matches any session) — they're
-- unreachable by every read path, and carrying them over would violate the
-- new FK at copy time.

DROP TABLE IF EXISTS tasks_new;
DROP TABLE IF EXISTS cost_summaries_new;

CREATE TABLE tasks_new (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    subject     TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'pending',
    active_form TEXT,
    blocked_by  TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
INSERT INTO tasks_new (id, session_id, subject, description, status, active_form, blocked_by, created_at, updated_at)
    SELECT t.id, t.session_id, t.subject, t.description, t.status, t.active_form, t.blocked_by, t.created_at, t.updated_at
    FROM tasks t
    JOIN sessions s ON s.id = t.session_id;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id);

CREATE TABLE cost_summaries_new (
    session_id    TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read    INTEGER NOT NULL DEFAULT 0,
    cache_write   INTEGER NOT NULL DEFAULT 0,
    total_usd     REAL NOT NULL DEFAULT 0.0,
    updated_at    INTEGER NOT NULL
);
INSERT INTO cost_summaries_new (session_id, input_tokens, output_tokens, cache_read, cache_write, total_usd, updated_at)
    SELECT c.session_id, c.input_tokens, c.output_tokens, c.cache_read, c.cache_write, c.total_usd, c.updated_at
    FROM cost_summaries c
    JOIN sessions s ON s.id = c.session_id;
DROP TABLE cost_summaries;
ALTER TABLE cost_summaries_new RENAME TO cost_summaries;
