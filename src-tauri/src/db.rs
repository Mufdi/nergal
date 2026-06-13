use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::agents::claude_code::cost::CostSummary;
use crate::models::{Session, SessionStatus, Workspace};
use crate::tasks::{Task, TaskStatus};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnnotationRow {
    pub id: String,
    pub session_id: String,
    pub ann_type: String,
    pub target: String,
    pub content: String,
    pub start_meta: String,
    pub end_meta: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpecAnnotationRow {
    pub id: String,
    pub spec_key: String,
    pub ann_type: String,
    pub target: String,
    pub content: String,
    pub start_meta: String,
    pub end_meta: String,
    pub created_at: String,
}

/// Thread-safe database handle managed as Tauri state.
pub type SharedDb = Arc<Mutex<Database>>;

pub struct Database {
    conn: Connection,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Parse the nullable `launch_options` JSON column. NULL, empty, or
/// malformed → `None` (a corrupt column must not break session loading).
fn parse_launch_options(raw: Option<String>) -> Option<crate::models::LaunchOptions> {
    let s = raw.filter(|s| !s.trim().is_empty())?;
    match serde_json::from_str(&s) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "malformed launch_options JSON; treating as none");
            None
        }
    }
}

/// Parse the nullable `env_shells` JSON column. NULL, empty, or malformed →
/// empty vec (a corrupt column must not break session loading).
fn parse_env_shells(raw: Option<String>) -> Vec<crate::models::EnvShellDef> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "malformed env_shells JSON; treating as empty");
            Vec::new()
        }
    }
}

/// Parse the nullable `pinned_note_paths` JSON-array column into a `Vec`.
/// NULL, empty, or malformed → empty vec (a corrupt column must not break
/// session loading).
fn parse_pinned_note_paths(raw: Option<String>) -> Vec<String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => {
            // A corrupt column drops this session from hot-reload silently
            // otherwise; surface it so it's diagnosable.
            tracing::warn!(error = %e, "malformed pinned_note_paths JSON; treating as empty");
            Vec::new()
        }
    }
}

/// Parse the nullable `pinned_clickup_task_ids` JSON-array column into a
/// `Vec`. NULL, empty, or malformed → empty vec (a corrupt column must not
/// break session loading).
fn parse_pinned_clickup_task_ids(raw: Option<String>) -> Vec<String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "malformed pinned_clickup_task_ids JSON; treating as empty");
            Vec::new()
        }
    }
}

impl Database {
    /// Raw connection access for the ClickUp reconcile: the `clickup::mirror`
    /// helpers take `&Connection` so a whole poll cycle can commit in one
    /// `unchecked_transaction` (atomicity is the spec's core promise there).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// In-memory database with all migrations applied, for tests outside this
    /// module (the assembler tests in `pty.rs` need a real `Database`).
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open (or create) the database at the standard config path.
    pub fn open() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        let db_dir = config_dir.join("cluihud");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("cluihud.db");

        let conn = Connection::open(&db_path)
            .with_context(|| format!("opening database: {}", db_path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Self { conn };
        db.migrate()?;
        db.migrate_from_json()?;

        Ok(db)
    }

    /// Run all pending migrations.
    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)",
        )?;

        let current: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let migrations: &[&str] = &[
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_merge_target.sql"),
            include_str!("../migrations/003_annotations.sql"),
            include_str!("../migrations/004_annotation_highlight_source.sql"),
            include_str!("../migrations/005_spec_annotations.sql"),
            include_str!("../migrations/006_scratchpad.sql"),
            include_str!("../migrations/007_agent_id.sql"),
            include_str!("../migrations/008_obsidian_config.sql"),
            include_str!("../migrations/009_obsidian_search_subdir.sql"),
            include_str!("../migrations/010_pinned_notes.sql"),
            include_str!("../migrations/011_launch_options.sql"),
            include_str!("../migrations/012_workspace_openspec_dir.sql"),
            include_str!("../migrations/013_env_shells.sql"),
            include_str!("../migrations/014_env_shell_suggestions.sql"),
            include_str!("../migrations/015_clickup_mirror.sql"),
            include_str!("../migrations/016_clickup_stale_since.sql"),
            include_str!("../migrations/017_clickup_user_id.sql"),
            include_str!("../migrations/018_clickup_session_binding.sql"),
            include_str!("../migrations/019_clickup_closed_out.sql"),
        ];

        for (i, sql) in migrations.iter().enumerate() {
            let version = (i + 1) as i64;
            if version > current {
                self.conn.execute_batch(sql)?;
                self.conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?1)",
                    [version],
                )?;
                tracing::info!("applied migration v{version}");
            }
        }

        Ok(())
    }

    /// One-shot import from state.json if it exists.
    fn migrate_from_json(&self) -> Result<()> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        let json_path = config_dir.join("cluihud").join("state.json");

        if !json_path.exists() {
            return Ok(());
        }

        // Check if we already have data
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM workspaces", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        tracing::info!("migrating state.json to SQLite...");

        #[derive(serde::Deserialize)]
        struct OldState {
            workspaces: Vec<serde_json::Value>,
        }

        let contents = std::fs::read_to_string(&json_path)?;
        let old: OldState = serde_json::from_str(&contents)?;

        let tx = self.conn.unchecked_transaction()?;

        for ws in &old.workspaces {
            let id = ws["id"].as_str().unwrap_or_default();
            let name = ws["name"].as_str().unwrap_or_default();
            let repo_path = ws["repo_path"].as_str().unwrap_or_default();
            let created_at = ws["created_at"].as_u64().unwrap_or(0);

            tx.execute(
                "INSERT OR IGNORE INTO workspaces (id, name, repo_path, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, name, repo_path, created_at],
            )?;

            if let Some(sessions) = ws["sessions"].as_array() {
                for s in sessions {
                    let sid = s["id"].as_str().unwrap_or_default();
                    let sname = s["name"].as_str().unwrap_or_default();
                    let wt_path = s["worktree_path"].as_str();
                    let wt_branch = s["worktree_branch"].as_str();
                    let status = s["status"].as_str().unwrap_or("idle");
                    let created = s["created_at"].as_u64().unwrap_or(0);
                    let updated = s["updated_at"].as_u64().unwrap_or(0);

                    tx.execute(
                        "INSERT OR IGNORE INTO sessions (id, workspace_id, name, worktree_path, worktree_branch, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![sid, id, sname, wt_path, wt_branch, status, created, updated],
                    )?;
                }
            }
        }

        tx.commit()?;

        // Rename so we don't re-import
        let migrated = json_path.with_extension("json.migrated");
        let _ = std::fs::rename(&json_path, &migrated);
        tracing::info!("state.json migrated to SQLite, renamed to .migrated");

        Ok(())
    }

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
            "SELECT id, name, repo_path, created_at FROM workspaces ORDER BY created_at",
        )?;
        let mut sess_stmt = self
            .conn
            .prepare("SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, pinned_note_paths, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids FROM sessions WHERE workspace_id = ?1 ORDER BY created_at")?;

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

    // ── Sessions ──

    pub fn create_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
            ],
        )?;
        Ok(())
    }

    pub fn find_session(&self, id: &str) -> Result<Option<Session>> {
        let result = self.conn.query_row(
            "SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at, agent_id, agent_internal_session_id, pinned_note_paths, launch_options, env_shells, active_clickup_task_id, pinned_clickup_task_ids FROM sessions WHERE id = ?1",
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
    /// so resume flows can pass it back via `--session <id>` after a cluihud
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

    // ── Annotations ──

    #[allow(clippy::too_many_arguments)] // Mirrors the column list 1:1; collapsing to a struct adds noise.
    pub fn save_annotation(
        &self,
        id: &str,
        session_id: &str,
        ann_type: &str,
        target: &str,
        content: &str,
        start_meta: &str,
        end_meta: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO annotations (id, session_id, type, target, content, start_meta, end_meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session_id, ann_type, target, content, start_meta, end_meta],
        )?;
        Ok(())
    }

    pub fn get_annotations(&self, session_id: &str) -> Result<Vec<AnnotationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, type, target, content, start_meta, end_meta, created_at FROM annotations WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([session_id], |row| {
                Ok(AnnotationRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    ann_type: row.get(2)?,
                    target: row.get(3)?,
                    content: row.get(4)?,
                    start_meta: row.get(5)?,
                    end_meta: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn delete_annotation(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM annotations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_annotations(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM annotations WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(())
    }

    // ── Spec annotations ──

    #[allow(clippy::too_many_arguments)] // Mirrors the column list 1:1; collapsing to a struct adds noise.
    pub fn save_spec_annotation(
        &self,
        id: &str,
        spec_key: &str,
        ann_type: &str,
        target: &str,
        content: &str,
        start_meta: &str,
        end_meta: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO spec_annotations (id, spec_key, type, target, content, start_meta, end_meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, spec_key, ann_type, target, content, start_meta, end_meta],
        )?;
        Ok(())
    }

    pub fn get_spec_annotations(&self, spec_key: &str) -> Result<Vec<SpecAnnotationRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, spec_key, type, target, content, start_meta, end_meta, created_at FROM spec_annotations WHERE spec_key = ?1 ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([spec_key], |row| {
                Ok(SpecAnnotationRow {
                    id: row.get(0)?,
                    spec_key: row.get(1)?,
                    ann_type: row.get(2)?,
                    target: row.get(3)?,
                    content: row.get(4)?,
                    start_meta: row.get(5)?,
                    end_meta: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn delete_spec_annotation(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM spec_annotations WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_spec_annotations(&self, spec_key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM spec_annotations WHERE spec_key = ?1",
            [spec_key],
        )?;
        Ok(())
    }

    /// Group annotation counts by spec_key for a given LIKE prefix (e.g. "my-change/%").
    pub fn count_spec_annotations_by_prefix(
        &self,
        prefix_like: &str,
    ) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT spec_key, COUNT(*) FROM spec_annotations WHERE spec_key LIKE ?1 GROUP BY spec_key",
        )?;
        let rows = stmt
            .query_map([prefix_like], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    // ── Scratchpad ──

    /// Upsert a scratchpad tab's metadata. `last_modified` is bumped to now.
    pub fn upsert_scratchpad_meta(&self, tab_id: &str, position: i64) -> Result<()> {
        let now = now_secs() as i64;
        self.conn.execute(
            "INSERT INTO scratchpad_meta (tab_id, position, created_at, last_modified) \
             VALUES (?1, ?2, ?3, ?3) \
             ON CONFLICT(tab_id) DO UPDATE SET position=?2, last_modified=?3",
            params![tab_id, position, now],
        )?;
        Ok(())
    }

    pub fn delete_scratchpad_meta(&self, tab_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM scratchpad_meta WHERE tab_id = ?1", [tab_id])?;
        Ok(())
    }

    pub fn list_scratchpad_meta(&self) -> Result<Vec<(String, i64, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tab_id, position, created_at, last_modified FROM scratchpad_meta \
             ORDER BY position ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Replace all scratchpad metadata in a single transaction (used on
    /// path change and watcher-driven reconciliation).
    pub fn replace_scratchpad_meta(&self, entries: &[(String, i64)]) -> Result<()> {
        let now = now_secs() as i64;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM scratchpad_meta", [])?;
        for (tab_id, position) in entries {
            tx.execute(
                "INSERT INTO scratchpad_meta (tab_id, position, created_at, last_modified) \
                 VALUES (?1, ?2, ?3, ?3)",
                params![tab_id, position, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove rows whose tab_id is not in the provided set (zombie cleanup
    /// after watcher emit).
    pub fn prune_scratchpad_meta(&self, keep: &[String]) -> Result<()> {
        if keep.is_empty() {
            self.conn.execute("DELETE FROM scratchpad_meta", [])?;
            return Ok(());
        }
        let placeholders = std::iter::repeat_n("?", keep.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM scratchpad_meta WHERE tab_id NOT IN ({placeholders})");
        let params: Vec<&dyn rusqlite::ToSql> =
            keep.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        self.conn.execute(&sql, params.as_slice())?;
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

    // ── Obsidian Config (obsidian-bridge change, migration 008) ──

    pub fn get_obsidian_config(
        &self,
        workspace_id: &str,
    ) -> Result<Option<crate::obsidian::config::ObsidianConfig>> {
        let result = self.conn.query_row(
            "SELECT vault_root, vault_name, session_log_path, quick_capture_path, \
                    moc_path, templates_path, backlinks_enabled, render_wikilinks, \
                    search_subdir \
             FROM obsidian_config WHERE workspace_id = ?1",
            [workspace_id],
            |r| {
                Ok(crate::obsidian::config::ObsidianConfig {
                    vault_root: r.get::<_, Option<String>>(0)?,
                    vault_name: r.get::<_, Option<String>>(1)?,
                    session_log_path: r.get::<_, Option<String>>(2)?,
                    quick_capture_path: r.get::<_, Option<String>>(3)?,
                    moc_path: r.get::<_, Option<String>>(4)?,
                    templates_path: r.get::<_, Option<String>>(5)?,
                    backlinks_enabled: r.get::<_, i64>(6)? != 0,
                    render_wikilinks: r.get::<_, i64>(7)? != 0,
                    search_subdir: r.get::<_, Option<String>>(8)?,
                })
            },
        );
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_obsidian_config(
        &self,
        workspace_id: &str,
        cfg: &crate::obsidian::config::ObsidianConfig,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO obsidian_config (workspace_id, vault_root, vault_name, \
                session_log_path, quick_capture_path, moc_path, templates_path, \
                backlinks_enabled, render_wikilinks, search_subdir, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
             ON CONFLICT(workspace_id) DO UPDATE SET \
                vault_root=?2, vault_name=?3, session_log_path=?4, \
                quick_capture_path=?5, moc_path=?6, templates_path=?7, \
                backlinks_enabled=?8, render_wikilinks=?9, search_subdir=?10, \
                updated_at=?11",
            params![
                workspace_id,
                cfg.vault_root,
                cfg.vault_name,
                cfg.session_log_path,
                cfg.quick_capture_path,
                cfg.moc_path,
                cfg.templates_path,
                cfg.backlinks_enabled as i64,
                cfg.render_wikilinks as i64,
                cfg.search_subdir,
                now_secs(),
            ],
        )?;
        Ok(())
    }

    pub fn delete_obsidian_config(&self, workspace_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM obsidian_config WHERE workspace_id = ?1",
            [workspace_id],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Session, SessionStatus};

    fn in_memory() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let db = Database { conn };
        db.migrate().unwrap();
        db
    }

    fn seed_session(db: &Database, sid: &str) {
        db.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        let s = Session {
            id: sid.to_string(),
            name: "s".into(),
            workspace_id: "ws1".into(),
            worktree_path: None,
            worktree_branch: None,
            merge_target: None,
            status: SessionStatus::Idle,
            created_at: 0,
            updated_at: 0,
            agent_id: "claude-code".into(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: None,
            env_shells: Vec::new(),
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
        };
        db.create_session(&s).unwrap();
    }

    #[test]
    fn clickup_mirror_migration_applies_on_fresh_db() {
        let db = in_memory();
        let expected = [
            "clickup_spaces",
            "clickup_folders",
            "clickup_lists",
            "clickup_statuses",
            "clickup_tasks",
            "clickup_custom_field_defs",
            "clickup_task_custom_values",
            "clickup_checklists",
            "clickup_checklist_items",
            "clickup_comments",
            "clickup_attachments",
            "clickup_sync_state",
        ];
        for table in expected {
            let count: i64 = db
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "missing table {table}");
        }
    }

    #[test]
    fn env_shells_round_trip() {
        let db = in_memory();
        // Seeds ws1 plus a defs-less session for the NULL-column case below.
        seed_session(&db, "s-none");
        let defs = vec![
            crate::models::EnvShellDef {
                label: "dev".into(),
                command: "pnpm dev".into(),
                cwd: None,
            },
            crate::models::EnvShellDef {
                label: "db".into(),
                command: "docker compose up".into(),
                cwd: Some("../backend".into()),
            },
        ];
        let s = Session {
            id: "s-env".to_string(),
            name: "s".into(),
            workspace_id: "ws1".into(),
            worktree_path: None,
            worktree_branch: None,
            merge_target: None,
            status: SessionStatus::Idle,
            created_at: 0,
            updated_at: 0,
            agent_id: "claude-code".into(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: None,
            env_shells: defs.clone(),
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
        };
        db.create_session(&s).unwrap();
        let loaded = db.find_session("s-env").unwrap().unwrap();
        assert_eq!(loaded.env_shells, defs);

        // No defs → NULL column → empty vec on load.
        let none = db.find_session("s-none").unwrap().unwrap();
        assert!(none.env_shells.is_empty());
    }

    #[test]
    fn workspace_openspec_dir_override() {
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        // Default: none.
        assert!(db.get_workspace_openspec_dir("ws1").unwrap().is_none());
        // Set + read back.
        db.set_workspace_openspec_dir("ws1", Some("/specs/ws1"))
            .unwrap();
        assert_eq!(
            db.get_workspace_openspec_dir("ws1").unwrap().as_deref(),
            Some("/specs/ws1")
        );
        // Empty string clears to default.
        db.set_workspace_openspec_dir("ws1", Some("  ")).unwrap();
        assert!(db.get_workspace_openspec_dir("ws1").unwrap().is_none());
    }

    #[test]
    fn env_shell_suggestions_round_trip() {
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        assert!(
            db.get_workspace_env_shell_suggestions("ws1")
                .unwrap()
                .is_empty()
        );
        let defs = vec![crate::models::EnvShellDef {
            label: "dev".into(),
            command: "pnpm dev".into(),
            cwd: None,
        }];
        db.set_workspace_env_shell_suggestions("ws1", &defs)
            .unwrap();
        assert_eq!(db.get_workspace_env_shell_suggestions("ws1").unwrap(), defs);

        // Coexists with the openspec_dir override on the same row.
        db.set_workspace_openspec_dir("ws1", Some("/specs/ws1"))
            .unwrap();
        assert_eq!(db.get_workspace_env_shell_suggestions("ws1").unwrap(), defs);

        // Empty list clears the column.
        db.set_workspace_env_shell_suggestions("ws1", &[]).unwrap();
        assert!(
            db.get_workspace_env_shell_suggestions("ws1")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn launch_options_round_trip() {
        use crate::models::{LaunchOptions, PermissionPreset};
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        let s = Session {
            id: "s-lo".to_string(),
            name: "s".into(),
            workspace_id: "ws1".into(),
            worktree_path: None,
            worktree_branch: None,
            merge_target: None,
            status: SessionStatus::Idle,
            created_at: 0,
            updated_at: 0,
            agent_id: "claude-code".into(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: Some(LaunchOptions {
                permission_preset: PermissionPreset::AcceptEdits,
                allow_skip_in_cycle: false,
                startup_command: Some("nvm use 20".into()),
            }),
            env_shells: Vec::new(),
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
        };
        db.create_session(&s).unwrap();
        let loaded = db.find_session("s-lo").unwrap().unwrap();
        let opts = loaded.launch_options.unwrap();
        assert_eq!(opts.permission_preset, PermissionPreset::AcceptEdits);
        assert_eq!(opts.startup_command.as_deref(), Some("nvm use 20"));
    }

    #[test]
    fn parse_launch_options_handles_null_and_garbage() {
        assert!(parse_launch_options(None).is_none());
        assert!(parse_launch_options(Some("".into())).is_none());
        assert!(parse_launch_options(Some("not json".into())).is_none());
    }

    #[test]
    fn parse_pinned_handles_null_and_garbage() {
        assert!(parse_pinned_note_paths(None).is_empty());
        assert!(parse_pinned_note_paths(Some("".into())).is_empty());
        assert!(parse_pinned_note_paths(Some("not json".into())).is_empty());
        assert_eq!(
            parse_pinned_note_paths(Some(r#"["/a.md","/b.md"]"#.into())),
            vec!["/a.md".to_string(), "/b.md".to_string()]
        );
    }

    #[test]
    fn pinned_notes_round_trip_with_dedup_and_order() {
        let db = in_memory();
        seed_session(&db, "sess");
        assert!(db.get_pinned_notes("sess").unwrap().is_empty());

        db.add_pinned_note("sess", "/vault/a.md").unwrap();
        db.add_pinned_note("sess", "/vault/b.md").unwrap();
        db.add_pinned_note("sess", "/vault/a.md").unwrap(); // dup → no-op
        assert_eq!(
            db.get_pinned_notes("sess").unwrap(),
            vec!["/vault/a.md".to_string(), "/vault/b.md".to_string()]
        );

        db.remove_pinned_note("sess", "/vault/a.md").unwrap();
        assert_eq!(
            db.get_pinned_notes("sess").unwrap(),
            vec!["/vault/b.md".to_string()]
        );
    }

    #[test]
    fn pinned_notes_survive_find_session() {
        let db = in_memory();
        seed_session(&db, "sess");
        db.add_pinned_note("sess", "/vault/x.md").unwrap();
        let loaded = db.find_session("sess").unwrap().unwrap();
        assert_eq!(loaded.pinned_note_paths, vec!["/vault/x.md".to_string()]);
    }

    #[test]
    fn parse_pinned_clickup_task_ids_handles_null_and_garbage() {
        assert!(parse_pinned_clickup_task_ids(None).is_empty());
        assert!(parse_pinned_clickup_task_ids(Some("".into())).is_empty());
        assert!(parse_pinned_clickup_task_ids(Some("not json".into())).is_empty());
        assert_eq!(
            parse_pinned_clickup_task_ids(Some(r#"["t1","t2"]"#.into())),
            vec!["t1".to_string(), "t2".to_string()]
        );
    }

    #[test]
    fn clickup_binding_survives_find_session() {
        let db = in_memory();
        seed_session(&db, "sess");

        // Fresh session: unbound, no pins.
        let fresh = db.find_session("sess").unwrap().unwrap();
        assert!(fresh.active_clickup_task_id.is_none());
        assert!(fresh.pinned_clickup_task_ids.is_empty());

        db.conn
            .execute(
                "UPDATE sessions SET active_clickup_task_id = 'tA', \
                 pinned_clickup_task_ids = '[\"tA\",\"tB\"]' WHERE id = 'sess'",
                [],
            )
            .unwrap();
        let loaded = db.find_session("sess").unwrap().unwrap();
        assert_eq!(loaded.active_clickup_task_id.as_deref(), Some("tA"));
        assert_eq!(
            loaded.pinned_clickup_task_ids,
            vec!["tA".to_string(), "tB".to_string()]
        );

        // The pre-joined workspace load carries the binding too.
        let workspaces = db.get_workspaces().unwrap();
        let session = &workspaces[0].sessions[0];
        assert_eq!(session.active_clickup_task_id.as_deref(), Some("tA"));
        assert_eq!(session.pinned_clickup_task_ids.len(), 2);
    }

    /// create_session's INSERT must carry the clickup columns so callers that
    /// pass pre-populated bindings (e.g. future restore or import flows) don't
    /// silently lose them. The clickup_spawn_worktree_with_task path used to
    /// require a separate UPDATE call precisely because the INSERT dropped them.
    #[test]
    fn create_session_persists_clickup_fields() {
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp").unwrap();
        let session = Session {
            id: "s-cu".into(),
            name: "cu".into(),
            workspace_id: "ws1".into(),
            worktree_path: None,
            worktree_branch: None,
            merge_target: None,
            status: SessionStatus::Idle,
            created_at: 0,
            updated_at: 0,
            agent_id: "claude-code".into(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: None,
            env_shells: Vec::new(),
            active_clickup_task_id: Some("task-42".into()),
            pinned_clickup_task_ids: vec!["task-7".into(), "task-99".into()],
        };
        db.create_session(&session).unwrap();
        let loaded = db.find_session("s-cu").unwrap().unwrap();
        assert_eq!(loaded.active_clickup_task_id.as_deref(), Some("task-42"));
        assert_eq!(
            loaded.pinned_clickup_task_ids,
            vec!["task-7".to_string(), "task-99".to_string()]
        );
    }
}
