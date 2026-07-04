use super::*;

impl Database {
    // ── Workspaces ──

    pub fn create_workspace(&self, id: &str, name: &str, repo_path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO workspaces (id, name, repo_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, repo_path, now_secs()],
        )?;
        Ok(())
    }

    /// Get all workspaces with their sessions pre-joined.
    pub fn get_workspaces(&self) -> Result<Vec<Workspace>> {
        let mut ws_stmt = self.conn.prepare(
            "SELECT id, name, repo_path, created_at FROM workspaces ORDER BY sort_order ASC, created_at ASC",
        )?;
        let mut sess_stmt = self
            .conn
            .prepare("SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, pinned_note_paths, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids, active_linear_issue_id, pinned_linear_issue_ids FROM sessions WHERE workspace_id = ?1 ORDER BY created_at")?;

        let workspaces = ws_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;

        let mut result = Vec::new();
        for ws in workspaces {
            let (id, name, repo_path, created_at) = ws?;

            let sessions: Vec<Session> = sess_stmt
                .query_map([&id], |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        name: row.get(2)?,
                        worktree_path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
                        worktree_branch: row.get(4)?,
                        merge_target: row.get(5)?,
                        status: SessionStatus::from_str(
                            &row.get::<_, String>(6).unwrap_or_default(),
                        ),
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
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            let repo_path = PathBuf::from(repo_path);
            result.push(Workspace {
                id,
                name,
                is_git: crate::worktree::is_git_repo(&repo_path),
                repo_path,
                sessions,
                created_at,
            });
        }

        Ok(result)
    }

    pub fn delete_workspace(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Rewrite sort_order for every id in `ordered_ids` in a single transaction.
    /// Ids not in the list are left at their current sort_order (e.g. rows added
    /// between the frontend fetch and this call).
    pub fn set_workspace_order(&self, ordered_ids: &[String]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (idx, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE workspaces SET sort_order = ?1 WHERE id = ?2",
                params![idx as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn workspace_repo_path(&self, id: &str) -> Result<Option<PathBuf>> {
        let result = self.conn.query_row(
            "SELECT repo_path FROM workspaces WHERE id = ?1",
            [id],
            |r| r.get::<_, String>(0),
        );
        match result {
            Ok(p) => Ok(Some(PathBuf::from(p))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
