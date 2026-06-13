-- Linear-style status glyphs need each task's workflow `type` (open / custom /
-- done / closed) to pick the icon shape, and ClickUp's optional `custom_id`
-- (the human "DEV-142" identifier) to surface a readable task id. Both are
-- additive, nullable: pre-existing rows render the legacy color dot / internal
-- id until the next poll re-populates them.
ALTER TABLE clickup_tasks ADD COLUMN status_type TEXT;
ALTER TABLE clickup_tasks ADD COLUMN custom_id TEXT;
