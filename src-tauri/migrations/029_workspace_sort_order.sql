ALTER TABLE workspaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

-- Backfill so existing rows keep their current created_at order. Ties broken by id.
UPDATE workspaces SET sort_order = (
    SELECT COUNT(*)
    FROM workspaces w2
    WHERE w2.created_at < workspaces.created_at
       OR (w2.created_at = workspaces.created_at AND w2.id < workspaces.id)
);
