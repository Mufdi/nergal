-- cross-session-messaging: cluihud-owned agent-to-agent message store.
--
-- Renumbered from the planned `015` (tasks §1.1): change 1 (cluihud-mcp-server)
-- and the Linear changes consumed 015..027, so the next free version is 028.
--
-- Two tables, schema byte-aligned with tasks §1.1 (iprev round 4):
--   - cross_session_threads: one row per conversation. Budget is a message-count
--     cap (msg_budget) + a wall-clock deadline (deadline_at) — NOT tokens, which
--     cluihud cannot measure inside an agent turn. `participants` is a JSON array
--     of session ids; `max_hops` bounds REACH (new participants), not turns.
--   - cross_session_messages: one row per relayed message. `depth` is per-message
--     reach (computed sender_depth + new-participant?1:0), not a thread scalar.
--     `agent_consumed_at` (set by read_messages) and `human_seen_at` (set by the
--     UI) are SEPARATE so a user glancing at the panel never cancels delivery.
--
-- No FK to sessions(id): the message store is a durable audit log that must
-- survive a session being deleted/forgotten (the history panel renders past
-- conversations). Only thread_id is FK-constrained.

CREATE TABLE IF NOT EXISTS cross_session_threads (
    id                 TEXT PRIMARY KEY,
    originator_session TEXT NOT NULL,
    participants       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'active',
    max_hops           INTEGER NOT NULL,
    msg_count          INTEGER NOT NULL DEFAULT 0,
    msg_budget         INTEGER,
    deadline_at        INTEGER,
    created_at         INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cross_session_messages (
    id                TEXT PRIMARY KEY,
    thread_id         TEXT NOT NULL REFERENCES cross_session_threads(id),
    from_session      TEXT NOT NULL,
    to_session        TEXT NOT NULL,
    body              TEXT NOT NULL,
    depth             INTEGER NOT NULL,
    dedup_key         TEXT NOT NULL,
    agent_consumed_at INTEGER,
    human_seen_at     INTEGER,
    created_at        INTEGER NOT NULL
);

-- Delivery query: undelivered-for-session is `WHERE to_session = ? AND
-- agent_consumed_at IS NULL`; the index makes it a covered lookup.
CREATE INDEX IF NOT EXISTS idx_cross_session_messages_delivery
    ON cross_session_messages (to_session, agent_consumed_at);
