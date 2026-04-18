CREATE TABLE IF NOT EXISTS spec_annotations (
    id TEXT PRIMARY KEY,
    spec_key TEXT NOT NULL,
    type TEXT NOT NULL CHECK(type IN ('comment', 'replace', 'delete', 'insert')),
    target TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    start_meta TEXT NOT NULL DEFAULT '{}',
    end_meta TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_spec_annotations_key ON spec_annotations(spec_key);
