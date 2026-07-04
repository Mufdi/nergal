use super::*;

impl Database {
    /// Upsert the AI summary for a session (phase 6). A row exists only when
    /// summaries are enabled for the session's project.
    ///
    /// `updated_at` is the **consumed** activity timestamp (the marker's
    /// `last_stop_at` at generation start), NOT wall-clock now — passed in by
    /// the runner so a Stop that lands mid-generation keeps the session dirty
    /// (`last_stop_at > updated_at`) and the final turn is not lost.
    pub fn set_session_summary(
        &self,
        session_id: &str,
        summary: &str,
        model: Option<&str>,
        token_cost: Option<i64>,
        updated_at: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_summaries (session_id, summary, model, token_cost, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
               summary = excluded.summary,
               model = excluded.model,
               token_cost = excluded.token_cost,
               updated_at = excluded.updated_at",
            params![session_id, summary, model, token_cost, updated_at],
        )?;
        Ok(())
    }

    /// Upsert the durable pull-marker for a session (Revision 1). Cheap, LLM-free,
    /// written on every `Stop` regardless of the summary opt-in.
    pub fn set_session_transcript(
        &self,
        session_id: &str,
        transcript_path: &str,
        last_stop_at: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_transcripts (session_id, transcript_path, last_stop_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
               transcript_path = excluded.transcript_path,
               last_stop_at = excluded.last_stop_at",
            params![session_id, transcript_path, last_stop_at],
        )?;
        Ok(())
    }

    /// Read a session's pull-marker, or `None` when it never produced a `Stop`.
    pub fn get_session_transcript(&self, session_id: &str) -> Result<Option<SessionTranscript>> {
        let result = self.conn.query_row(
            "SELECT transcript_path, last_stop_at FROM session_transcripts WHERE session_id = ?1",
            [session_id],
            |row| {
                Ok(SessionTranscript {
                    transcript_path: row.get(0)?,
                    last_stop_at: row.get(1)?,
                })
            },
        );
        match result {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// All pull-markers keyed by session id. Folded into the MCP directory
    /// snapshot (parallel to `get_all_session_summaries`) so `list_sessions`
    /// resolves every session's `last_stop_at` in one query under the brief lock.
    pub fn get_all_session_transcripts(
        &self,
    ) -> Result<std::collections::HashMap<String, SessionTranscript>> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id, transcript_path, last_stop_at FROM session_transcripts")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SessionTranscript {
                    transcript_path: row.get(1)?,
                    last_stop_at: row.get(2)?,
                },
            ))
        })?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (id, t) = r?;
            map.insert(id, t);
        }
        Ok(map)
    }

    /// All session summaries keyed by session id. Used to enrich the MCP
    /// directory in one query while the brief snapshot lock is held.
    pub fn get_all_session_summaries(
        &self,
    ) -> Result<std::collections::HashMap<String, SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, summary, model, token_cost, updated_at FROM session_summaries",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SessionSummary {
                    summary: row.get(1)?,
                    model: row.get(2)?,
                    token_cost: row.get(3)?,
                    updated_at: row.get(4)?,
                },
            ))
        })?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (id, s) = r?;
            map.insert(id, s);
        }
        Ok(map)
    }

    /// Read a session's AI summary, or `None` when never summarized.
    pub fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let result = self.conn.query_row(
            "SELECT summary, model, token_cost, updated_at FROM session_summaries WHERE session_id = ?1",
            [session_id],
            |row| {
                Ok(SessionSummary {
                    summary: row.get(0)?,
                    model: row.get(1)?,
                    token_cost: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
