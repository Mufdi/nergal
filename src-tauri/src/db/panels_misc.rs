use super::*;

impl Database {
    // ── Pinned vault notes ──

    pub fn get_pinned_notes(&self, session_id: &str) -> Result<Vec<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT pinned_note_paths FROM sessions WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        Ok(parse_pinned_note_paths(raw))
    }

    /// Append `path` to a session's pinned notes. Idempotent: an already-pinned
    /// path is a no-op so order stays stable and the agent isn't re-fed dupes.
    pub fn add_pinned_note(&self, session_id: &str, path: &str) -> Result<()> {
        let mut paths = self.get_pinned_notes(session_id)?;
        if paths.iter().any(|p| p == path) {
            return Ok(());
        }
        paths.push(path.to_string());
        self.write_pinned_notes(session_id, &paths)
    }

    pub fn remove_pinned_note(&self, session_id: &str, path: &str) -> Result<()> {
        let mut paths = self.get_pinned_notes(session_id)?;
        paths.retain(|p| p != path);
        self.write_pinned_notes(session_id, &paths)
    }

    /// Every session that has at least one pinned note, with its paths. Feeds
    /// the hot-reload watcher's union of watched files.
    pub fn all_pinned_notes(&self) -> Result<Vec<(String, Vec<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, pinned_note_paths FROM sessions \
             WHERE pinned_note_paths IS NOT NULL AND pinned_note_paths != ''",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, parse_pinned_note_paths(r.get(1)?)))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, paths) = row?;
            if !paths.is_empty() {
                out.push((id, paths));
            }
        }
        Ok(out)
    }

    fn write_pinned_notes(&self, session_id: &str, paths: &[String]) -> Result<()> {
        let json = serde_json::to_string(paths).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "UPDATE sessions SET pinned_note_paths = ?1, updated_at = ?2 WHERE id = ?3",
            params![json, now_secs(), session_id],
        )?;
        Ok(())
    }

    // ── ClickUp session binding (clickup-task-integration) ──

    /// Set (or clear with `None`) the session's single active ClickUp task —
    /// the write-back target. Binding over an existing task replaces it; the
    /// UI confirms the replacement upstream.
    pub fn set_active_clickup_task(&self, session_id: &str, task_id: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET active_clickup_task_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![task_id, now_secs(), session_id],
        )?;
        Ok(())
    }

    pub fn get_pinned_clickup_tasks(&self, session_id: &str) -> Result<Vec<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT pinned_clickup_task_ids FROM sessions WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        Ok(parse_pinned_clickup_task_ids(raw))
    }

    /// Append `task_id` to a session's pinned ClickUp tasks. Idempotent: an
    /// already-pinned id is a no-op so order stays stable (mirrors
    /// `add_pinned_note`).
    pub fn add_pinned_clickup_task(&self, session_id: &str, task_id: &str) -> Result<()> {
        let mut ids = self.get_pinned_clickup_tasks(session_id)?;
        if ids.iter().any(|t| t == task_id) {
            return Ok(());
        }
        ids.push(task_id.to_string());
        self.write_pinned_clickup_tasks(session_id, &ids)
    }

    pub fn remove_pinned_clickup_task(&self, session_id: &str, task_id: &str) -> Result<()> {
        let mut ids = self.get_pinned_clickup_tasks(session_id)?;
        ids.retain(|t| t != task_id);
        self.write_pinned_clickup_tasks(session_id, &ids)
    }

    fn write_pinned_clickup_tasks(&self, session_id: &str, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "UPDATE sessions SET pinned_clickup_task_ids = ?1, updated_at = ?2 WHERE id = ?3",
            params![json, now_secs(), session_id],
        )?;
        Ok(())
    }

    /// Set (or clear with `None`) the session's single active Linear issue —
    /// the write-back target. Binding over an existing issue replaces it; the
    /// UI confirms the replacement upstream. (Mirrors `set_active_clickup_task`.)
    pub fn set_active_linear_issue(&self, session_id: &str, issue_id: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET active_linear_issue_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![issue_id, now_secs(), session_id],
        )?;
        Ok(())
    }

    pub fn get_pinned_linear_issues(&self, session_id: &str) -> Result<Vec<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT pinned_linear_issue_ids FROM sessions WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        Ok(parse_pinned_linear_issue_ids(raw))
    }

    /// Append `issue_id` to a session's pinned Linear issues. Idempotent: an
    /// already-pinned id is a no-op so order stays stable (mirrors
    /// `add_pinned_clickup_task`).
    pub fn add_pinned_linear_issue(&self, session_id: &str, issue_id: &str) -> Result<()> {
        let mut ids = self.get_pinned_linear_issues(session_id)?;
        if ids.iter().any(|t| t == issue_id) {
            return Ok(());
        }
        ids.push(issue_id.to_string());
        self.write_pinned_linear_issues(session_id, &ids)
    }

    pub fn remove_pinned_linear_issue(&self, session_id: &str, issue_id: &str) -> Result<()> {
        let mut ids = self.get_pinned_linear_issues(session_id)?;
        ids.retain(|t| t != issue_id);
        self.write_pinned_linear_issues(session_id, &ids)
    }

    fn write_pinned_linear_issues(&self, session_id: &str, ids: &[String]) -> Result<()> {
        let json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "UPDATE sessions SET pinned_linear_issue_ids = ?1, updated_at = ?2 WHERE id = ?3",
            params![json, now_secs(), session_id],
        )?;
        Ok(())
    }

    // ── Floating panel geometry (multi-row, keyed by panel_id) ──

    pub fn get_panel_geometry(&self, panel_id: &str) -> Result<Option<(String, f64)>> {
        let result = self.conn.query_row(
            "SELECT geometry_json, opacity FROM floating_panel_geometry WHERE panel_id = ?1",
            [panel_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_panel_geometry(
        &self,
        panel_id: &str,
        geometry_json: &str,
        opacity: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO floating_panel_geometry (panel_id, geometry_json, opacity) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(panel_id) DO UPDATE SET geometry_json=?2, opacity=?3",
            params![panel_id, geometry_json, opacity],
        )?;
        Ok(())
    }

    // ── Per-workspace OpenSpec dir override ──

    /// The configured OpenSpec directory for a workspace, or `None` when it
    /// uses the default (`<repo>/openspec`). Empty string is treated as None.
    pub fn get_workspace_openspec_dir(&self, workspace_id: &str) -> Result<Option<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT openspec_dir FROM workspace_config WHERE workspace_id = ?1",
                [workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(raw.filter(|s| !s.trim().is_empty()))
    }

    /// Set (or clear, with `None`) the OpenSpec dir override for a workspace.
    pub fn set_workspace_openspec_dir(
        &self,
        workspace_id: &str,
        openspec_dir: Option<&str>,
    ) -> Result<()> {
        let value = openspec_dir.map(str::trim).filter(|s| !s.is_empty());
        self.conn.execute(
            "INSERT INTO workspace_config (workspace_id, openspec_dir, updated_at) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(workspace_id) DO UPDATE SET openspec_dir=?2, updated_at=?3",
            params![workspace_id, value, now_secs()],
        )?;
        Ok(())
    }

    // ── Per-workspace plans-dir override ──

    /// The configured plans directory for a workspace, or `None` when it uses
    /// the auto-resolved default. Empty string is treated as None.
    pub fn get_workspace_plans_dir(&self, workspace_id: &str) -> Result<Option<String>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT plans_dir FROM workspace_config WHERE workspace_id = ?1",
                [workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(raw.filter(|s| !s.trim().is_empty()))
    }

    /// Set (or clear, with `None`) the plans-dir override for a workspace.
    pub fn set_workspace_plans_dir(
        &self,
        workspace_id: &str,
        plans_dir: Option<&str>,
    ) -> Result<()> {
        let value = plans_dir.map(str::trim).filter(|s| !s.is_empty());
        self.conn.execute(
            "INSERT INTO workspace_config (workspace_id, plans_dir, updated_at) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(workspace_id) DO UPDATE SET plans_dir=?2, updated_at=?3",
            params![workspace_id, value, now_secs()],
        )?;
        Ok(())
    }

    // ── Per-workspace environment-shell suggestions ──

    pub fn get_workspace_env_shell_suggestions(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<crate::models::EnvShellDef>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT env_shell_suggestions FROM workspace_config WHERE workspace_id = ?1",
                [workspace_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(parse_env_shells(raw))
    }

    pub fn set_workspace_env_shell_suggestions(
        &self,
        workspace_id: &str,
        suggestions: &[crate::models::EnvShellDef],
    ) -> Result<()> {
        let value = if suggestions.is_empty() {
            None
        } else {
            serde_json::to_string(suggestions).ok()
        };
        self.conn.execute(
            "INSERT INTO workspace_config (workspace_id, env_shell_suggestions, updated_at) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(workspace_id) DO UPDATE SET env_shell_suggestions=?2, updated_at=?3",
            params![workspace_id, value, now_secs()],
        )?;
        Ok(())
    }
}
