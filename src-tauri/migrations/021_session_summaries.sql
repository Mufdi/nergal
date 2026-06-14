-- Opt-in AI session summaries (cluihud-mcp-server change, session-summary
-- capability). One row per session, written ONLY when summaries are enabled
-- (global or per-project). Absence of a row = never summarized / disabled.
-- `token_cost` is the accounted output+input tokens of the summarizer call.
CREATE TABLE IF NOT EXISTS session_summaries (
    session_id TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    model TEXT,
    token_cost INTEGER,
    updated_at INTEGER NOT NULL
);
