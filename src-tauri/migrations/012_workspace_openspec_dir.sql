-- Per-workspace OpenSpec directory override. NULL = default (<repo>/openspec).
-- Lets specs live outside the code repo (e.g. a sibling dir) so the repo stays
-- clean. ON DELETE CASCADE: workspace deletion drops the row; the user's spec
-- files stay untouched (they live wherever the override points).
CREATE TABLE IF NOT EXISTS workspace_config (
    workspace_id  TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    openspec_dir  TEXT,
    updated_at    INTEGER NOT NULL
);
