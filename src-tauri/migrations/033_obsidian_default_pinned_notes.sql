-- default_pinned_note_paths: per-workspace JSON array of vault-relative note
-- paths that every NEW session in the workspace auto-pins on creation, so a
-- fresh session starts with the workspace's standing context. NULL/empty = none.
-- Same single-column-JSON pattern as sessions.pinned_note_paths (migration 010).
ALTER TABLE obsidian_config ADD COLUMN default_pinned_note_paths TEXT;
