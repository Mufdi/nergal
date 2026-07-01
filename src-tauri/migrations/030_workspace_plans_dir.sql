-- Per-workspace plans directory override. NULL = auto-resolved default.
-- Additive: tells Nergal where to look, does not change where CC writes.
ALTER TABLE workspace_config ADD COLUMN plans_dir TEXT;
