-- Session ↔ Linear issue binding (linear-agent-integration). active = the
-- single write-back target; pinned = context-only issue ids, a JSON array with
-- the same single-column pattern as pinned_clickup_task_ids (018). NULL =
-- unbound / no pins.
ALTER TABLE sessions ADD COLUMN active_linear_issue_id TEXT;
ALTER TABLE sessions ADD COLUMN pinned_linear_issue_ids TEXT;
