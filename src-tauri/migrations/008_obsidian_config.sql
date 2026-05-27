-- ON DELETE CASCADE: workspace deletion drops the row; user's vault files
-- stay untouched because they live outside the cluihud config dir.

CREATE TABLE IF NOT EXISTS obsidian_config (
    workspace_id          TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    vault_root            TEXT,
    vault_name            TEXT,
    session_log_path      TEXT,
    quick_capture_path    TEXT,
    moc_path              TEXT,
    templates_path        TEXT,
    backlinks_enabled     INTEGER NOT NULL DEFAULT 0,
    render_wikilinks      INTEGER NOT NULL DEFAULT 1,
    updated_at            INTEGER NOT NULL
);
