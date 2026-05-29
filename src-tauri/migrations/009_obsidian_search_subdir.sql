-- search_subdir: optional vault-relative folder that scopes vault search + the
-- @@ picker. NULL/empty = whole vault. Preserves the channel-registry premise
-- (no prescribed layout) — users with their own Obsidian structure scope to a
-- folder they pick, the toggle in the search modal flips between the two.
ALTER TABLE obsidian_config ADD COLUMN search_subdir TEXT;
