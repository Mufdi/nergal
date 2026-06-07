-- Per-session environment-shell definitions (label + command), JSON array.
-- Spawned as quake shells: auto-run at session creation, pre-filled on
-- re-open after a restart.
ALTER TABLE sessions ADD COLUMN env_shells TEXT;
