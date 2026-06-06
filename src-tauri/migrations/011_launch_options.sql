-- Per-session launch options (permission preset + startup command) as a
-- JSON object; NULL for sessions created without explicit options.
ALTER TABLE sessions ADD COLUMN launch_options TEXT;
