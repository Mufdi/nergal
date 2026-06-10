-- ClickUp read mirror (clickup-sync change). Structure-agnostic tree:
-- Space → Folder → List → Task → Subtask with real FKs; vocabularies
-- (per-List statuses, custom-field defs) are rows, not enums, so workspace
-- structure changes are absorbed as data. Folderless Lists reference a
-- folder row flagged hidden=1. `stale` is the tombstone bit: absent-from-
-- fetch rows are marked, never hard-deleted mid-iteration.

CREATE TABLE IF NOT EXISTS clickup_spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    synced_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS clickup_folders (
    id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL REFERENCES clickup_spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0,
    stale INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS clickup_lists (
    id TEXT PRIMARY KEY,
    folder_id TEXT REFERENCES clickup_folders(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES clickup_spaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    stale INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS clickup_statuses (
    id TEXT PRIMARY KEY,
    list_id TEXT NOT NULL REFERENCES clickup_lists(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    color TEXT,
    orderindex INTEGER,
    type TEXT
);

CREATE TABLE IF NOT EXISTS clickup_tasks (
    id TEXT PRIMARY KEY,
    list_id TEXT NOT NULL REFERENCES clickup_lists(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES clickup_tasks(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    text_content TEXT,
    status_name TEXT,
    status_color TEXT,
    priority TEXT,
    assignees_json TEXT NOT NULL DEFAULT '[]',
    tags_json TEXT NOT NULL DEFAULT '[]',
    due_date INTEGER,
    start_date INTEGER,
    date_created INTEGER,
    date_updated INTEGER,
    url TEXT,
    archived INTEGER NOT NULL DEFAULT 0,
    stale INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_clickup_tasks_list ON clickup_tasks(list_id);
CREATE INDEX IF NOT EXISTS idx_clickup_tasks_parent ON clickup_tasks(parent_id);
CREATE INDEX IF NOT EXISTS idx_clickup_tasks_stale ON clickup_tasks(stale);

CREATE TABLE IF NOT EXISTS clickup_custom_field_defs (
    id TEXT PRIMARY KEY,
    scope_level TEXT,
    scope_id TEXT,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    type_config_json TEXT
);

CREATE TABLE IF NOT EXISTS clickup_task_custom_values (
    task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE,
    field_id TEXT NOT NULL REFERENCES clickup_custom_field_defs(id) ON DELETE CASCADE,
    value_json TEXT,
    PRIMARY KEY (task_id, field_id)
);

CREATE TABLE IF NOT EXISTS clickup_checklists (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE,
    name TEXT,
    orderindex INTEGER
);

CREATE TABLE IF NOT EXISTS clickup_checklist_items (
    id TEXT PRIMARY KEY,
    checklist_id TEXT NOT NULL REFERENCES clickup_checklists(id) ON DELETE CASCADE,
    name TEXT,
    resolved INTEGER NOT NULL DEFAULT 0,
    orderindex INTEGER
);

CREATE TABLE IF NOT EXISTS clickup_comments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE,
    user_json TEXT,
    text TEXT,
    date INTEGER,
    resolved INTEGER NOT NULL DEFAULT 0,
    reply_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS clickup_attachments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES clickup_tasks(id) ON DELETE CASCADE,
    title TEXT,
    url TEXT,
    mimetype TEXT,
    size INTEGER,
    thumbnail_url TEXT
);

-- Silent-first-sync gate: notifications arm only after baseline_done=1.
CREATE TABLE IF NOT EXISTS clickup_sync_state (
    team_id TEXT PRIMARY KEY,
    baseline_done INTEGER NOT NULL DEFAULT 0,
    last_full_sync INTEGER
);
