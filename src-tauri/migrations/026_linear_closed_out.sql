-- "Worked & closed" marker for Linear issues (linear-writeback change).
-- Local-only, intentionally SEPARATE from any Linear state: it records that
-- this issue was closed out from a Nergal session, surviving restarts and
-- independent of whatever state the issue carries in Linear. Keyed by issue id;
-- closed_at is Unix seconds.
CREATE TABLE IF NOT EXISTS linear_closed_out (
    issue_id  TEXT PRIMARY KEY,
    closed_at INTEGER NOT NULL
);
