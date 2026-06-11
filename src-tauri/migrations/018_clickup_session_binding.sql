-- Session ↔ ClickUp task binding (clickup-task-integration). active = the
-- single write-back target; pinned = context-only task ids, a JSON array with
-- the same single-column pattern as pinned_note_paths (010). NULL = unbound /
-- no pins.
ALTER TABLE sessions ADD COLUMN active_clickup_task_id TEXT;
ALTER TABLE sessions ADD COLUMN pinned_clickup_task_ids TEXT;
