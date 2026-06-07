-- Per-workspace library of suggested environment shells (label + command),
-- JSON array. Quick-picked in the new-session modal; stacks differ between
-- workspaces so the library is scoped, not global.
ALTER TABLE workspace_config ADD COLUMN env_shell_suggestions TEXT;
