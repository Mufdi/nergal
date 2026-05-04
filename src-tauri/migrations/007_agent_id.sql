-- Foundation for agent-agnostic sessions.
--
-- agent_id: stable adapter identifier. NOT NULL with default 'claude-code'
-- so existing rows backfill without intervention; the column constraints
-- (matching the AgentId regex `^[a-z][a-z0-9-]{0,31}$`) live in Rust.
--
-- agent_internal_session_id: nullable. Pi/Codex carry their own session
-- UUIDs distinct from cluihud's session_id; CC currently doesn't need it
-- (uses --continue with no id). Pre-foundation rows leave it NULL.

ALTER TABLE sessions ADD COLUMN agent_id TEXT NOT NULL DEFAULT 'claude-code';
ALTER TABLE sessions ADD COLUMN agent_internal_session_id TEXT;

CREATE INDEX IF NOT EXISTS idx_sessions_agent_id ON sessions(agent_id);
