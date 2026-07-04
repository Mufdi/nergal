use super::*;

impl Database {
    // ── Tasks ──

    pub fn upsert_task(&self, session_id: &str, task: &Task) -> Result<()> {
        let blocked_by_json = serde_json::to_string(&task.blocked_by).unwrap_or_default();
        let status = match task.status {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Deleted => "deleted",
        };
        let now = now_secs();
        self.conn.execute(
            "INSERT INTO tasks (id, session_id, subject, description, status, active_form, blocked_by, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(id) DO UPDATE SET subject=?3, description=?4, status=?5, active_form=?6, blocked_by=?7, updated_at=?8",
            params![task.id, session_id, task.subject, task.description, status, task.active_form, blocked_by_json, now],
        )?;
        Ok(())
    }

    /// Tombstone every completed task of a session so a clear survives reload
    /// (BUG-12). 'deleted' rows are filtered out of `get_visible_tasks`.
    pub fn mark_completed_tasks_deleted(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET status = 'deleted', updated_at = ?2 WHERE session_id = ?1 AND status = 'completed'",
            params![session_id, now_secs()],
        )?;
        Ok(())
    }

    /// Tombstone a single task by id.
    pub fn mark_task_deleted(&self, session_id: &str, task_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET status = 'deleted', updated_at = ?3 WHERE session_id = ?1 AND id = ?2",
            params![session_id, task_id, now_secs()],
        )?;
        Ok(())
    }

    /// Tombstone every task of a session (force-clear, incl. ghosts CC no longer
    /// tracks). The TodoWrite sync skips user-tombstoned ids, so they don't
    /// resurrect.
    pub fn mark_all_tasks_deleted(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET status = 'deleted', updated_at = ?2 WHERE session_id = ?1 AND status != 'deleted'",
            params![session_id, now_secs()],
        )?;
        Ok(())
    }

    pub fn get_visible_tasks(&self, session_id: &str) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, subject, description, status, active_form, blocked_by FROM tasks WHERE session_id = ?1 AND status != 'deleted' ORDER BY created_at",
        )?;
        let tasks = stmt
            .query_map([session_id], |row| {
                let status_str: String = row.get(3)?;
                let blocked_by_json: String = row.get(5)?;
                Ok(Task {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    description: row.get(2)?,
                    status: match status_str.as_str() {
                        "in_progress" => TaskStatus::InProgress,
                        "completed" => TaskStatus::Completed,
                        "deleted" => TaskStatus::Deleted,
                        _ => TaskStatus::Pending,
                    },
                    active_form: row.get(4)?,
                    blocked_by: serde_json::from_str(&blocked_by_json).unwrap_or_default(),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    /// IDs of user-tombstoned tasks for a session. Used by the TodoWrite sync
    /// to avoid resurrecting tasks the user explicitly cleared — CC may still
    /// carry them in its in-memory todo list after the user hit "Clear completed".
    pub fn get_tombstoned_task_ids(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM tasks WHERE session_id = ?1 AND status = 'deleted'")?;
        let ids = stmt
            .query_map([session_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    // ── Costs ──

    pub fn upsert_cost(&self, session_id: &str, cost: &CostSummary) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cost_summaries (session_id, input_tokens, output_tokens, cache_read, cache_write, total_usd, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(session_id) DO UPDATE SET input_tokens=?2, output_tokens=?3, cache_read=?4, cache_write=?5, total_usd=?6, updated_at=?7",
            params![session_id, cost.input_tokens, cost.output_tokens, cost.cache_read_tokens, cost.cache_write_tokens, cost.total_usd, now_secs()],
        )?;
        Ok(())
    }

    pub fn get_cost(&self, session_id: &str) -> Result<Option<CostSummary>> {
        let result = self.conn.query_row(
            "SELECT input_tokens, output_tokens, cache_read, cache_write, total_usd FROM cost_summaries WHERE session_id = ?1",
            [session_id],
            |r| Ok(CostSummary {
                input_tokens: r.get(0)?,
                output_tokens: r.get(1)?,
                cache_read_tokens: r.get(2)?,
                cache_write_tokens: r.get(3)?,
                total_usd: r.get(4)?,
            }),
        );
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
