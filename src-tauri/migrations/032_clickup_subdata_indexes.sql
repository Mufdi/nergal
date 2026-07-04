-- The ClickUp mirror's per-task subdata tables (015_clickup_mirror.sql) shipped
-- with only PRIMARY KEY (id), so read_tasks' correlated COUNT(*) subqueries and
-- the detail view's comments lookup ran full scans per task on every panel mount
-- and each 45s poll refresh. These indexes back those task_id lookups.
CREATE INDEX IF NOT EXISTS idx_clickup_checklists_task ON clickup_checklists(task_id);
CREATE INDEX IF NOT EXISTS idx_clickup_attachments_task ON clickup_attachments(task_id);
CREATE INDEX IF NOT EXISTS idx_clickup_comments_task ON clickup_comments(task_id);
