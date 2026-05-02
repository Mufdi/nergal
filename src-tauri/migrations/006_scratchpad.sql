CREATE TABLE IF NOT EXISTS scratchpad_meta (
    tab_id TEXT PRIMARY KEY,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    last_modified INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scratchpad_meta_position
    ON scratchpad_meta(position);

CREATE TABLE IF NOT EXISTS floating_panel_geometry (
    panel_id TEXT PRIMARY KEY,
    geometry_json TEXT NOT NULL,
    opacity REAL NOT NULL DEFAULT 0.9
);
