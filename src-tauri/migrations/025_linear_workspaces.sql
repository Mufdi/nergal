-- Multi-workspace (linear-mirror-enhancements). The mirror stays single-tenant:
-- one ACTIVE workspace reflected at a time. Secrets live in the keyring (one
-- account per org: `linear-token::<org_id>`); this table holds only non-secret
-- metadata for the Settings list. `active_org_id` names the mirrored workspace;
-- switching it bumps the key-generation epoch + wipes the mirror (see mod.rs).
CREATE TABLE IF NOT EXISTS linear_workspaces (
    org_id   TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    url_key  TEXT,
    added_at INTEGER NOT NULL
);

ALTER TABLE linear_sync_state ADD COLUMN active_org_id TEXT;
