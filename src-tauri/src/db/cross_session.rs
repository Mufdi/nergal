use super::*;

impl Database {
    // -- cross-session-messaging store (thin primitives; router/dedup/cap logic
    //    lives in `mcp::router` so it stays pure and unit-testable). --

    /// Insert a freshly created thread.
    pub fn insert_cross_session_thread(&self, t: &CrossSessionThread) -> Result<()> {
        let participants = serde_json::to_string(&t.participants)?;
        self.conn.execute(
            "INSERT INTO cross_session_threads
               (id, originator_session, participants, status, max_hops, msg_count, msg_budget, deadline_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                t.id,
                t.originator_session,
                participants,
                t.status,
                t.max_hops,
                t.msg_count,
                t.msg_budget,
                t.deadline_at,
                t.created_at,
            ],
        )?;
        Ok(())
    }

    fn row_to_cross_session_thread(row: &rusqlite::Row) -> rusqlite::Result<CrossSessionThread> {
        let participants_json: String = row.get(2)?;
        let participants: Vec<String> =
            serde_json::from_str(&participants_json).unwrap_or_default();
        Ok(CrossSessionThread {
            id: row.get(0)?,
            originator_session: row.get(1)?,
            participants,
            status: row.get(3)?,
            max_hops: row.get(4)?,
            msg_count: row.get(5)?,
            msg_budget: row.get(6)?,
            deadline_at: row.get(7)?,
            created_at: row.get(8)?,
        })
    }

    const CROSS_THREAD_COLS: &'static str = "id, originator_session, participants, status, max_hops, msg_count, msg_budget, deadline_at, created_at";

    pub fn get_cross_session_thread(&self, id: &str) -> Result<Option<CrossSessionThread>> {
        let sql = format!(
            "SELECT {} FROM cross_session_threads WHERE id = ?1",
            Self::CROSS_THREAD_COLS
        );
        let res = self
            .conn
            .query_row(&sql, [id], Self::row_to_cross_session_thread);
        match res {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update a thread's mutable fields after a message lands (participant set
    /// may grow, msg_count increments, status may close on budget exhaustion).
    pub fn update_cross_session_thread(
        &self,
        id: &str,
        participants: &[String],
        status: &str,
        msg_count: u32,
    ) -> Result<()> {
        let participants_json = serde_json::to_string(participants)?;
        self.conn.execute(
            "UPDATE cross_session_threads SET participants = ?1, status = ?2, msg_count = ?3 WHERE id = ?4",
            params![participants_json, status, msg_count, id],
        )?;
        Ok(())
    }

    pub fn set_cross_session_thread_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE cross_session_threads SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    pub fn insert_cross_session_message(&self, m: &CrossSessionMessage) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cross_session_messages
               (id, thread_id, from_session, to_session, body, depth, dedup_key, agent_consumed_at, human_seen_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                m.id,
                m.thread_id,
                m.from_session,
                m.to_session,
                m.body,
                m.depth,
                m.dedup_key,
                m.agent_consumed_at,
                m.human_seen_at,
                m.created_at,
            ],
        )?;
        Ok(())
    }

    /// Whether an identical (from, to, normalized-body) message already exists in
    /// this thread — the conservative exact-match dedup backstop.
    pub fn cross_session_dedup_exists(&self, thread_id: &str, dedup_key: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM cross_session_messages WHERE thread_id = ?1 AND dedup_key = ?2",
            params![thread_id, dedup_key],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    fn row_to_cross_session_message(row: &rusqlite::Row) -> rusqlite::Result<CrossSessionMessage> {
        Ok(CrossSessionMessage {
            id: row.get(0)?,
            thread_id: row.get(1)?,
            from_session: row.get(2)?,
            to_session: row.get(3)?,
            body: row.get(4)?,
            depth: row.get(5)?,
            dedup_key: row.get(6)?,
            agent_consumed_at: row.get(7)?,
            human_seen_at: row.get(8)?,
            created_at: row.get(9)?,
        })
    }

    const CROSS_MSG_COLS: &'static str = "id, thread_id, from_session, to_session, body, depth, dedup_key, agent_consumed_at, human_seen_at, created_at";

    /// Mark messages agent-consumed (delivery satisfied) at `now`. Take-on-read:
    /// called by `read_messages`. Never touches `human_seen_at`.
    pub fn mark_cross_session_agent_consumed(&self, ids: &[String], now: u64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE cross_session_messages SET agent_consumed_at = ?1
                 WHERE id = ?2 AND agent_consumed_at IS NULL",
                params![now, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Mark every message in a thread human-seen (UI opened the thread) at `now`.
    /// Never touches `agent_consumed_at`, so the agent's delivery is never
    /// cancelled by a user glancing at the panel.
    pub fn mark_cross_session_human_seen(&self, thread_id: &str, now: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE cross_session_messages SET human_seen_at = ?1
             WHERE thread_id = ?2 AND human_seen_at IS NULL",
            params![now, thread_id],
        )?;
        Ok(())
    }

    /// Threads a session participates in, newest-first. Filtered in Rust (the set
    /// is small and the participant list is JSON) to avoid a JSON1 dependency.
    pub fn cross_session_threads_for(&self, session: &str) -> Result<Vec<CrossSessionThread>> {
        let sql = format!(
            "SELECT {} FROM cross_session_threads ORDER BY created_at DESC",
            Self::CROSS_THREAD_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::row_to_cross_session_thread)?;
        let mut out = Vec::new();
        for r in rows {
            let t = r?;
            if t.participants.iter().any(|p| p == session) {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// All threads (any participant), newest-first — the UI history panel roster.
    pub fn cross_session_all_threads(&self) -> Result<Vec<CrossSessionThread>> {
        let sql = format!(
            "SELECT {} FROM cross_session_threads ORDER BY created_at DESC",
            Self::CROSS_THREAD_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::row_to_cross_session_thread)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Every message in a thread, oldest-first (conversation order).
    pub fn cross_session_messages_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<CrossSessionMessage>> {
        let sql = format!(
            "SELECT {} FROM cross_session_messages WHERE thread_id = ?1 ORDER BY created_at ASC",
            Self::CROSS_MSG_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([thread_id], Self::row_to_cross_session_message)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Messages awaiting agent delivery for a session (`agent_consumed_at IS
    /// NULL`), oldest-first. Drives both `read_messages` and the idle-drain.
    pub fn cross_session_undelivered_for(&self, session: &str) -> Result<Vec<CrossSessionMessage>> {
        let sql = format!(
            "SELECT {} FROM cross_session_messages
             WHERE to_session = ?1 AND agent_consumed_at IS NULL ORDER BY created_at ASC",
            Self::CROSS_MSG_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([session], Self::row_to_cross_session_message)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Per-session count of messages the HUMAN has not yet seen
    /// (`human_seen_at IS NULL`), keyed by `to_session` — drives the SessionRow
    /// unread badge. Independent of `agent_consumed_at` (agent delivery).
    pub fn cross_session_human_unread_counts(
        &self,
    ) -> Result<std::collections::HashMap<String, u32>> {
        let mut stmt = self.conn.prepare(
            "SELECT to_session, COUNT(*) FROM cross_session_messages
             WHERE human_seen_at IS NULL GROUP BY to_session",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (id, n) = r?;
            map.insert(id, n);
        }
        Ok(map)
    }

    /// Active threads whose wall-clock deadline has passed — the sweeper's input.
    pub fn cross_session_threads_past_deadline(&self, now: u64) -> Result<Vec<CrossSessionThread>> {
        let sql = format!(
            "SELECT {} FROM cross_session_threads
             WHERE status = 'active' AND deadline_at IS NOT NULL AND deadline_at <= ?1",
            Self::CROSS_THREAD_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([now], Self::row_to_cross_session_thread)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}
