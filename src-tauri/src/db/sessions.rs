use super::*;

impl Database {
    // ── Sessions ──

    pub fn create_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids, active_linear_issue_id, pinned_linear_issue_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                session.id,
                session.workspace_id,
                session.name,
                session.worktree_path.as_ref().map(|p| p.display().to_string()),
                session.worktree_branch,
                session.merge_target,
                session.status.as_str(),
                session.created_at,
                session.updated_at,
                session.agent_id,
                session.agent_internal_session_id,
                session
                    .launch_options
                    .as_ref()
                    .and_then(|o| serde_json::to_string(o).ok()),
                if session.env_shells.is_empty() {
                    None
                } else {
                    serde_json::to_string(&session.env_shells).ok()
                },
                session.active_clickup_task_id.as_deref(),
                if session.pinned_clickup_task_ids.is_empty() {
                    None
                } else {
                    serde_json::to_string(&session.pinned_clickup_task_ids).ok()
                },
                session.active_linear_issue_id.as_deref(),
                if session.pinned_linear_issue_ids.is_empty() {
                    None
                } else {
                    serde_json::to_string(&session.pinned_linear_issue_ids).ok()
                },
            ],
        )?;
        Ok(())
    }

    pub fn find_session(&self, id: &str) -> Result<Option<Session>> {
        let result = self.conn.query_row(
            "SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, pinned_note_paths, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids, active_linear_issue_id, pinned_linear_issue_ids FROM sessions WHERE id = ?1",
            [id],
            |row| Ok(Session {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                name: row.get(2)?,
                worktree_path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
                worktree_branch: row.get(4)?,
                merge_target: row.get(5)?,
                status: SessionStatus::from_str(&row.get::<_, String>(6).unwrap_or_default()),
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                agent_id: row.get(9)?,
                agent_internal_session_id: row.get(10)?,
                agent_capabilities: Vec::new(),
                pinned_note_paths: parse_pinned_note_paths(row.get(11)?),
                launch_options: parse_launch_options(row.get(12)?),
                env_shells: parse_env_shells(row.get(13)?),
                active_clickup_task_id: row.get(14)?,
                pinned_clickup_task_ids: parse_pinned_clickup_task_ids(row.get(15)?),
                active_linear_issue_id: row.get(16)?,
                pinned_linear_issue_ids: parse_pinned_linear_issue_ids(row.get(17)?),
            }),
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn session_count_for_workspace(&self, workspace_id: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE workspace_id = ?1",
            [workspace_id],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn update_session_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now_secs(), id],
        )?;
        Ok(())
    }

    pub fn rename_session(&self, id: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now_secs(), id],
        )?;
        Ok(())
    }

    pub fn update_worktree_branch(&self, id: &str, branch: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET worktree_branch = ?1, updated_at = ?2 WHERE id = ?3",
            params![branch, now_secs(), id],
        )?;
        Ok(())
    }

    /// Persist the agent-internal session id (e.g. Pi UUID, Codex rollout id)
    /// so resume flows can pass it back via `--session <id>` after a nergal
    /// restart. Idempotent.
    pub fn update_agent_internal_session_id(&self, id: &str, internal_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET agent_internal_session_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![internal_id, now_secs(), id],
        )?;
        Ok(())
    }

    /// Persist the session's live quake tab set. The column seeds from the
    /// new-session modal defs, then evolves with use: ad-hoc tabs join,
    /// closed tabs leave, and each submitted command updates its shell —
    /// re-open recreates the set pre-filled.
    pub fn update_session_env_shells(
        &self,
        id: &str,
        defs: &[crate::models::EnvShellDef],
    ) -> Result<()> {
        let value = if defs.is_empty() {
            None
        } else {
            serde_json::to_string(defs).ok()
        };
        self.conn.execute(
            "UPDATE sessions SET env_shells = ?1, updated_at = ?2 WHERE id = ?3",
            params![value, now_secs(), id],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Get all sessions with worktree paths for reconciliation.
    pub fn sessions_with_worktrees(&self) -> Result<Vec<(String, PathBuf)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, worktree_path FROM sessions WHERE worktree_path IS NOT NULL")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .map(|(id, path)| (id, PathBuf::from(path)))
            .collect();
        Ok(rows)
    }

    pub fn set_merge_target(&self, id: &str, target: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET merge_target = ?1, updated_at = ?2 WHERE id = ?3",
            params![target, now_secs(), id],
        )?;
        Ok(())
    }

    pub fn clear_merge_target(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET merge_target = NULL, updated_at = ?1 WHERE id = ?2",
            params![now_secs(), id],
        )?;
        Ok(())
    }

    pub fn clear_session_worktree(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET worktree_path = NULL, worktree_branch = NULL, status = 'idle', updated_at = ?1 WHERE id = ?2",
            params![now_secs(), id],
        )?;
        Ok(())
    }
}
