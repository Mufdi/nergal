-- Linear read mirror (linear-mirror change #1 of 3). Linear's model is fixed-
-- schema (no ClickUp-style custom fields, no checklists): Team → Issue →
-- Sub-issue, with per-team workflow states (native `type`) and first-class
-- labels (def table + join). Vocabularies are rows, not enums, so adding a
-- state/label/team in Linear is absorbed as data.
--
-- Design notes forced by the bounded poll scope (iterative-plan-review):
--   * issues.parent_id is a PLAIN column, NOT a self-FK: a sub-issue's parent
--     can be out of the polled window, and page order (updatedAt desc) is not
--     topological — a hard self-FK would abort the insert. The tree is built in
--     app, tolerant of a dangling parent (rendered as a root).
--   * No `archived` column: Linear's issues connection excludes archived issues,
--     so an archived issue vanishes from the fetch and is evicted by absence;
--     a populated archived=1 state would never be observable.
--   * Only issues are tombstoned/evicted. Teams/states/labels/projects/cycles
--     are upsert-only and GC'd only when unreferenced by a live issue — never
--     absence-tombstoned. So a placeholder team/state row synthesized for an
--     unknown FK (marked synthetic=1 on those tables) can never oscillate
--     (its table isn't tombstoned); synthetic also lets the panel render a stub
--     and lets GC clean it once the real row arrives / it goes unreferenced.
--   * issues.was_viewer_assigned mirrors whether the row is believed the viewer's,
--     so the poller can re-verify exactly those by id (un-assignment detection
--     independent of whether an assignee change bumps updatedAt).
--   * labels are upsert-only (no stale columns): a per-team fetch is not
--     authoritative over workspace-scoped labels; defs are GC'd only when
--     unreferenced by any live issue.
--   * sync_state.key_generation is the account-swap epoch: set_key bumps it, and
--     a poll cycle aborts its commit if the generation changed mid-cycle.
--
-- foreign_keys=ON is already set on every connection (db.rs). `stale` is the
-- tombstone bit; `stale_since` records when a row went stale.

CREATE TABLE IF NOT EXISTS linear_teams (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    key       TEXT NOT NULL,
    synthetic INTEGER NOT NULL DEFAULT 0,
    synced_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS linear_workflow_states (
    id         TEXT PRIMARY KEY,
    team_id    TEXT NOT NULL REFERENCES linear_teams(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    type       TEXT NOT NULL,
    color      TEXT,
    position   REAL,
    synthetic  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS linear_labels (
    id      TEXT PRIMARY KEY,
    team_id TEXT REFERENCES linear_teams(id) ON DELETE CASCADE,
    name    TEXT NOT NULL,
    color   TEXT
);

CREATE TABLE IF NOT EXISTS linear_projects (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL,
    state TEXT
);

CREATE TABLE IF NOT EXISTS linear_cycles (
    id        TEXT PRIMARY KEY,
    team_id   TEXT NOT NULL REFERENCES linear_teams(id) ON DELETE CASCADE,
    number    INTEGER,
    name      TEXT,
    starts_at INTEGER,
    ends_at   INTEGER
);

CREATE TABLE IF NOT EXISTS linear_users (
    id           TEXT PRIMARY KEY,
    name         TEXT,
    display_name TEXT,
    email        TEXT,
    avatar_url   TEXT
);

CREATE TABLE IF NOT EXISTS linear_issues (
    id                  TEXT PRIMARY KEY,
    identifier          TEXT,
    team_id             TEXT NOT NULL REFERENCES linear_teams(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    description         TEXT,
    state_id            TEXT REFERENCES linear_workflow_states(id),
    priority            INTEGER NOT NULL DEFAULT 0,
    estimate            REAL,
    assignee_id         TEXT REFERENCES linear_users(id),
    project_id          TEXT REFERENCES linear_projects(id),
    cycle_id            TEXT REFERENCES linear_cycles(id),
    parent_id           TEXT,
    was_viewer_assigned INTEGER NOT NULL DEFAULT 0,
    due_date            INTEGER,
    created_at          INTEGER,
    updated_at          INTEGER,
    completed_at        INTEGER,
    url                 TEXT,
    stale               INTEGER NOT NULL DEFAULT 0,
    stale_since         INTEGER
);

CREATE INDEX IF NOT EXISTS idx_linear_issues_team ON linear_issues(team_id);
CREATE INDEX IF NOT EXISTS idx_linear_issues_parent ON linear_issues(parent_id);
CREATE INDEX IF NOT EXISTS idx_linear_issues_state ON linear_issues(state_id);
CREATE INDEX IF NOT EXISTS idx_linear_issues_stale ON linear_issues(stale);
CREATE INDEX IF NOT EXISTS idx_linear_issues_updated ON linear_issues(updated_at);

CREATE TABLE IF NOT EXISTS linear_issue_labels (
    issue_id TEXT NOT NULL REFERENCES linear_issues(id) ON DELETE CASCADE,
    label_id TEXT NOT NULL REFERENCES linear_labels(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_linear_issue_labels_label ON linear_issue_labels(label_id);

CREATE TABLE IF NOT EXISTS linear_comments (
    id         TEXT PRIMARY KEY,
    issue_id   TEXT NOT NULL REFERENCES linear_issues(id) ON DELETE CASCADE,
    user_json  TEXT,
    body       TEXT,
    created_at INTEGER
);

CREATE TABLE IF NOT EXISTS linear_sync_state (
    id                     INTEGER PRIMARY KEY CHECK (id = 1),
    baseline_done          INTEGER NOT NULL DEFAULT 0,
    last_full_sync         INTEGER,
    viewer_id              TEXT,
    selected_team_ids_json TEXT NOT NULL DEFAULT '[]',
    key_generation         INTEGER NOT NULL DEFAULT 0
);

-- Seed the single sync-state row so the poller can UPDATE it unconditionally.
INSERT OR IGNORE INTO linear_sync_state (id) VALUES (1);
