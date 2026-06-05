-- pinned_note_paths: JSON array of absolute vault-note paths pinned to a
-- session. Bodies are assembled into the agent's context at spawn + resume
-- (obsidian-context-injection #3/#H). NULL/empty = no pins. Same single-column
-- JSON pattern as tasks.blocked_by.
ALTER TABLE sessions ADD COLUMN pinned_note_paths TEXT;
