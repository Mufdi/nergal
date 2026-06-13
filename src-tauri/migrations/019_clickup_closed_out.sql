-- "Worked & closed" marker (clickup-writeback walk follow-up). Local-only and
-- intentionally SEPARATE from the ClickUp status: it records that this task was
-- closed out from a Nergal session, surviving restarts and independent of
-- whatever status the task carries in ClickUp. Keyed by task id; closed_at is
-- Unix seconds.
CREATE TABLE IF NOT EXISTS clickup_closed_out (
    task_id TEXT PRIMARY KEY,
    closed_at INTEGER NOT NULL
);
