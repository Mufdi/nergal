use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::claude::cost::CostSummary;
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

impl Database {
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
        let mut ws_stmt = self
            .conn
            .prepare("SELECT id, name, repo_path, created_at FROM workspaces ORDER BY created_at")?;
        let mut sess_stmt = self
            .conn
            .prepare("SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at FROM sessions WHERE workspace_id = ?1 ORDER BY created_at")?;

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
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            result.push(Workspace {
                id,
                name,
                repo_path: PathBuf::from(repo_path),
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
            "INSERT INTO sessions (id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
            ],
        )?;
        Ok(())
    }

    pub fn find_session(&self, id: &str) -> Result<Option<Session>> {
        let result = self.conn.query_row(
            "SELECT id, workspace_id, name, worktree_path, worktree_branch, merge_target, status, created_at, updated_at FROM sessions WHERE id = ?1",
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

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Get all sessions with worktree paths for reconciliation.
    pub fn sessions_with_worktrees(&self) -> Result<Vec<(String, PathBuf)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, worktree_path FROM sessions WHERE worktree_path IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
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
        self.conn
            .execute("DELETE FROM annotations WHERE session_id = ?1", [session_id])?;
        Ok(())
    }

    // ── Spec annotations ──

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
        self.conn
            .execute("DELETE FROM spec_annotations WHERE spec_key = ?1", [spec_key])?;
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
            .query_map([prefix_like], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}
