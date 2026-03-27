CREATE TABLE IF NOT EXISTS annotations (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK(type IN ('comment', 'replace', 'delete', 'insert')),
    target TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    position_start INTEGER NOT NULL DEFAULT 0,
    position_end INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_annotations_session ON annotations(session_id);
