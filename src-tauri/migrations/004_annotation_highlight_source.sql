-- Replace position_start/position_end with start_meta/end_meta (HighlightSource JSON)
-- SQLite doesn't support DROP COLUMN cleanly, so we rebuild the table.

CREATE TABLE IF NOT EXISTS annotations_new (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK(type IN ('comment', 'replace', 'delete', 'insert')),
    target TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    start_meta TEXT NOT NULL DEFAULT '{}',
    end_meta TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO annotations_new (id, session_id, type, target, content, start_meta, end_meta, created_at)
SELECT id, session_id, type, target, content, '{}', '{}', created_at
FROM annotations;

DROP TABLE IF EXISTS annotations;
ALTER TABLE annotations_new RENAME TO annotations;

CREATE INDEX IF NOT EXISTS idx_annotations_session ON annotations(session_id);
