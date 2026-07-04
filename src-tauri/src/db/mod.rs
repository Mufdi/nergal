use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::agents::claude_code::cost::CostSummary;
use crate::models::{Session, SessionStatus, Workspace};
use crate::tasks::{Task, TaskStatus};

/// Persisted AI session summary (phase 6). A row exists only when summaries
/// are enabled for the session's project; absence means never-summarized.
///
/// `updated_at` denotes the **activity covered through** (the consumed
/// `last_stop_at`), not the generation wall-clock — so the dirty check
/// (`last_stop_at > updated_at`) stays correct when a Stop lands mid-generation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSummary {
    pub summary: String,
    pub model: Option<String>,
    pub token_cost: Option<i64>,
    pub updated_at: u64,
}

/// Durable pull-marker for lazy summary generation (Revision 1). Written cheaply
/// (no LLM) on every `Stop`; lets the read path locate a session's transcript
/// (live or recently-dead) and decide dirtiness.
#[derive(Debug, Clone)]
pub struct SessionTranscript {
    pub transcript_path: String,
    pub last_stop_at: u64,
}

/// One cross-session conversation thread (cross-session-messaging). `participants`
/// is the ordered set of session ids that have taken part; `max_hops` bounds
/// REACH (pulling in a new participant), while `msg_budget`/`deadline_at` bound
/// conversation length + wall-clock — budget is NEVER tokens (nergal cannot
/// measure agent-side tokens).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossSessionThread {
    pub id: String,
    pub originator_session: String,
    pub participants: Vec<String>,
    pub status: String,
    pub max_hops: u32,
    pub msg_count: u32,
    pub msg_budget: Option<u32>,
    pub deadline_at: Option<u64>,
    pub created_at: u64,
}

/// One relayed cross-session message. `depth` is per-message reach (not a thread
/// scalar); `agent_consumed_at` (set by `read_messages`, drives delivery) and
/// `human_seen_at` (set by the UI) are deliberately separate columns so a user
/// opening the panel never cancels a pending agent delivery.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossSessionMessage {
    pub id: String,
    pub thread_id: String,
    pub from_session: String,
    pub to_session: String,
    pub body: String,
    pub depth: u32,
    pub dedup_key: String,
    pub agent_consumed_at: Option<u64>,
    pub human_seen_at: Option<u64>,
    pub created_at: u64,
}

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

/// Parse the nullable `pinned_linear_issue_ids` JSON-array column into a `Vec`.
/// NULL, empty, or malformed → empty vec (mirrors `parse_pinned_clickup_task_ids`).
fn parse_pinned_linear_issue_ids(raw: Option<String>) -> Vec<String> {
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "malformed pinned_linear_issue_ids JSON; treating as empty");
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
        let db_dir = config_dir.join("nergal");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("nergal.db");

        let conn = Connection::open(&db_path)
            .with_context(|| format!("opening database: {}", db_path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let db = Self { conn };
        db.migrate()?;
        db.migrate_from_json()?;

        Ok(db)
    }

    /// Apply one migration's DDL and its `schema_version` bump as a single atomic
    /// unit: the runner owns the transaction so a crash or error mid-migration
    /// leaves neither applied, and the migration re-runs cleanly on next launch.
    /// Migration SQL files must NOT contain their own `BEGIN`/`COMMIT` (a nested
    /// transaction is a SQLite error). Statements that cannot run inside a
    /// transaction (`VACUUM`, some `PRAGMA`) are unsupported by this runner —
    /// none exist in the current migration set.
    fn apply_migration(conn: &Connection, version: i64, sql: &str) -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [version],
        )?;
        tx.commit()?;
        Ok(())
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
            include_str!("../../migrations/001_initial.sql"),
            include_str!("../../migrations/002_merge_target.sql"),
            include_str!("../../migrations/003_annotations.sql"),
            include_str!("../../migrations/004_annotation_highlight_source.sql"),
            include_str!("../../migrations/005_spec_annotations.sql"),
            include_str!("../../migrations/006_scratchpad.sql"),
            include_str!("../../migrations/007_agent_id.sql"),
            include_str!("../../migrations/008_obsidian_config.sql"),
            include_str!("../../migrations/009_obsidian_search_subdir.sql"),
            include_str!("../../migrations/010_pinned_notes.sql"),
            include_str!("../../migrations/011_launch_options.sql"),
            include_str!("../../migrations/012_workspace_openspec_dir.sql"),
            include_str!("../../migrations/013_env_shells.sql"),
            include_str!("../../migrations/014_env_shell_suggestions.sql"),
            include_str!("../../migrations/015_clickup_mirror.sql"),
            include_str!("../../migrations/016_clickup_stale_since.sql"),
            include_str!("../../migrations/017_clickup_user_id.sql"),
            include_str!("../../migrations/018_clickup_session_binding.sql"),
            include_str!("../../migrations/019_clickup_closed_out.sql"),
            include_str!("../../migrations/020_clickup_status_type.sql"),
            include_str!("../../migrations/021_session_summaries.sql"),
            include_str!("../../migrations/022_session_transcripts.sql"),
            include_str!("../../migrations/023_linear_mirror.sql"),
            include_str!("../../migrations/024_linear_session_binding.sql"),
            include_str!("../../migrations/025_linear_workspaces.sql"),
            include_str!("../../migrations/026_linear_closed_out.sql"),
            include_str!("../../migrations/027_linear_estimation_type.sql"),
            include_str!("../../migrations/028_cross_session.sql"),
            include_str!("../../migrations/029_workspace_sort_order.sql"),
            include_str!("../../migrations/030_workspace_plans_dir.sql"),
            include_str!("../../migrations/031_session_child_fk_cascade.sql"),
            include_str!("../../migrations/032_clickup_subdata_indexes.sql"),
        ];

        for (i, sql) in migrations.iter().enumerate() {
            let version = (i + 1) as i64;
            if version > current {
                Self::apply_migration(&self.conn, version, sql)?;
                tracing::info!("applied migration v{version}");
            }
        }

        Ok(())
    }

    /// One-shot import from state.json if it exists.
    fn migrate_from_json(&self) -> Result<()> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"));
        let json_path = config_dir.join("nergal").join("state.json");

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
}

mod annotations;
mod cross_session;
mod obsidian;
mod panels_misc;
mod scratchpad;
mod sessions;
mod tasks_costs;
mod transcripts;
mod workspaces;

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
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
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
    fn session_summary_round_trips_and_upserts() {
        let db = in_memory();
        seed_session(&db, "s1");
        // Absent until written.
        assert!(db.get_session_summary("s1").unwrap().is_none());
        assert!(db.get_all_session_summaries().unwrap().is_empty());

        db.set_session_summary("s1", "did the thing", Some("gpt-4o-mini"), Some(123), 100)
            .unwrap();
        let got = db.get_session_summary("s1").unwrap().unwrap();
        assert_eq!(got.summary, "did the thing");
        assert_eq!(got.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(got.token_cost, Some(123));
        assert_eq!(got.updated_at, 100, "consumed timestamp stored verbatim");

        // Upsert overwrites; agent-CLI backend reports no token cost.
        db.set_session_summary("s1", "did it again", Some("claude"), None, 200)
            .unwrap();
        let got = db.get_session_summary("s1").unwrap().unwrap();
        assert_eq!(got.summary, "did it again");
        assert_eq!(got.token_cost, None);
        assert_eq!(got.updated_at, 200);

        let all = db.get_all_session_summaries().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all.get("s1").unwrap().summary, "did it again");
    }

    #[test]
    fn session_transcript_round_trips_and_upserts() {
        let db = in_memory();
        seed_session(&db, "s1");
        assert!(db.get_session_transcript("s1").unwrap().is_none());
        assert!(db.get_all_session_transcripts().unwrap().is_empty());

        db.set_session_transcript("s1", "/tmp/a.jsonl", 50).unwrap();
        let got = db.get_session_transcript("s1").unwrap().unwrap();
        assert_eq!(got.transcript_path, "/tmp/a.jsonl");
        assert_eq!(got.last_stop_at, 50);

        // Upsert advances the marker.
        db.set_session_transcript("s1", "/tmp/a.jsonl", 75).unwrap();
        assert_eq!(
            db.get_session_transcript("s1")
                .unwrap()
                .unwrap()
                .last_stop_at,
            75
        );
        assert_eq!(db.get_all_session_transcripts().unwrap().len(), 1);
    }

    #[test]
    fn cost_is_keyed_by_the_id_passed_in_not_a_second_id() {
        // Guards the id-resolution fix at the Stop-handler call site: cost
        // must be upserted under the Nergal session id, never the transient
        // CC-internal id, or a resume (new CC-internal id) would fragment
        // cost history across orphaned rows.
        let db = in_memory();
        let nergal_id = "nergal-s1";
        let cc_internal_id = "cc-internal-abc123";
        seed_session(&db, nergal_id);

        let cost = CostSummary {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            cache_write_tokens: 5,
            total_usd: 1.23,
        };

        // In the Stop handler the id is `nergal_session_id.unwrap_or(session_id)`;
        // here nergal_id stands in for a present nergal_session_id, so the cost
        // must land under it, never under the CC-internal id.
        let resolved_id = nergal_id;
        db.upsert_cost(resolved_id, &cost).unwrap();

        let got = db.get_cost(nergal_id).unwrap().unwrap();
        assert_eq!(got.input_tokens, 100);
        assert_eq!(got.total_usd, 1.23);
        assert!(
            db.get_cost(cc_internal_id).unwrap().is_none(),
            "cost must not land under the CC-internal id"
        );
    }

    #[test]
    fn deleting_session_cascades_summary_and_transcript() {
        let db = in_memory();
        seed_session(&db, "s1");
        db.set_session_summary("s1", "recap", None, None, 10)
            .unwrap();
        db.set_session_transcript("s1", "/tmp/s1.jsonl", 10)
            .unwrap();

        db.delete_session("s1").unwrap();

        // FK ON DELETE CASCADE removes both companion rows — no orphans.
        assert!(db.get_session_summary("s1").unwrap().is_none());
        assert!(db.get_session_transcript("s1").unwrap().is_none());
    }

    #[test]
    fn deleting_session_cascades_tasks_and_cost_summary() {
        let db = in_memory();
        seed_session(&db, "s1");
        db.conn
            .execute(
                "INSERT INTO tasks (id, session_id, subject, created_at, updated_at) VALUES ('t1', 's1', 'do it', 0, 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO cost_summaries (session_id, updated_at) VALUES ('s1', 0)",
                [],
            )
            .unwrap();

        db.delete_session("s1").unwrap();

        let tasks: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tasks, 0, "FK cascade must remove tasks on session delete");
        let costs: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM cost_summaries WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            costs, 0,
            "FK cascade must remove cost_summaries on session delete"
        );
    }

    #[test]
    fn deleting_workspace_cascades_to_session_tasks_and_cost_summary() {
        let db = in_memory();
        seed_session(&db, "s1");
        db.conn
            .execute(
                "INSERT INTO tasks (id, session_id, subject, created_at, updated_at) VALUES ('t1', 's1', 'do it', 0, 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO cost_summaries (session_id, updated_at) VALUES ('s1', 0)",
                [],
            )
            .unwrap();

        // No Rust helper touches sessions/tasks/cost_summaries here — only the
        // workspace delete, which must cascade through sessions (001 FK) to
        // tasks/cost_summaries (031 FK) at the SQL layer alone.
        db.conn
            .execute("DELETE FROM workspaces WHERE id = 'ws1'", [])
            .unwrap();

        let sessions: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE id = 's1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sessions, 0, "workspace delete must cascade to session");
        let tasks: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            tasks, 0,
            "workspace delete must cascade to grandchild tasks"
        );
        let costs: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM cost_summaries WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            costs, 0,
            "workspace delete must cascade to grandchild cost_summaries"
        );
    }

    #[test]
    fn migration_022_drops_orphan_summaries_on_rebuild() {
        // Simulate the 021 leak on a pre-022 DB: a summary row whose session was
        // deleted while 021 had no FK. 022's orphan-filtered copy must complete
        // and drop it (a blind SELECT * would FK-violate and abort).
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let db = Database { conn };
        // Apply only the prefix needed (001 creates sessions; 021 the FK-less
        // session_summaries), then inject an orphan as the 021 leak would.
        db.conn
            .execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        db.conn
            .execute_batch(include_str!("../../migrations/021_session_summaries.sql"))
            .unwrap();
        // Orphan: summary with no matching session row.
        db.conn
            .execute(
                "INSERT INTO session_summaries (session_id, summary, model, token_cost, updated_at)
                 VALUES ('ghost', 'orphan recap', NULL, NULL, 1)",
                [],
            )
            .unwrap();
        // Now run 022.
        db.conn
            .execute_batch(include_str!("../../migrations/022_session_transcripts.sql"))
            .unwrap();
        // The orphan is gone and the table now carries the FK.
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM session_summaries WHERE session_id = 'ghost'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "orphan summary dropped by 022 copy filter");
    }

    #[test]
    fn migration_022_is_idempotent_after_partial_apply() {
        // Simulate a partial 022 apply: a leftover session_summaries_new from a
        // crash mid-rebuild. The DROP IF EXISTS at the head must let 022 re-run.
        let db = in_memory(); // already at 022
        db.conn
            .execute_batch("CREATE TABLE session_summaries_new (x INTEGER)")
            .unwrap();
        // Re-running 022 must not error ("table already exists").
        db.conn
            .execute_batch(include_str!("../../migrations/022_session_transcripts.sql"))
            .unwrap();
        // session_summaries still usable after the re-run.
        seed_session(&db, "s1");
        db.set_session_summary("s1", "ok", None, None, 5).unwrap();
        assert_eq!(db.get_session_summary("s1").unwrap().unwrap().summary, "ok");
    }

    #[test]
    fn migration_031_drops_orphan_tasks_and_costs_and_enforces_fk() {
        // Simulate the pre-031 leak on a 001-only DB: a task/cost row whose
        // session doesn't exist, alongside a valid row. 031's orphan-filtered
        // copy must drop the former and preserve the latter's columns verbatim.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let db = Database { conn };
        db.conn
            .execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO workspaces (id, name, repo_path, created_at) VALUES ('ws1', 'ws', '/tmp/repo', 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sessions (id, workspace_id, name, status, created_at, updated_at) VALUES ('s1', 'ws1', 's', 'idle', 0, 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO tasks (id, session_id, subject, description, status, active_form, blocked_by, created_at, updated_at)
                 VALUES ('t1', 's1', 'valid task', 'desc', 'pending', 'doing', '[]', 1, 2)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO tasks (id, session_id, subject, created_at, updated_at) VALUES ('ghost', 'nope', 'orphan task', 0, 0)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO cost_summaries (session_id, input_tokens, updated_at) VALUES ('s1', 42, 3)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO cost_summaries (session_id, updated_at) VALUES ('nope', 0)",
                [],
            )
            .unwrap();

        db.conn
            .execute_batch(include_str!(
                "../../migrations/031_session_child_fk_cascade.sql"
            ))
            .unwrap();

        let ghost_tasks: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE id = 'ghost'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ghost_tasks, 0, "orphan task dropped by 031 copy filter");
        let ghost_costs: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM cost_summaries WHERE session_id = 'nope'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            ghost_costs, 0,
            "orphan cost_summaries dropped by 031 copy filter"
        );

        let (subject, description, status, active_form, blocked_by, created_at, updated_at): (
            String,
            String,
            String,
            Option<String>,
            String,
            i64,
            i64,
        ) = db
            .conn
            .query_row(
                "SELECT subject, description, status, active_form, blocked_by, created_at, updated_at FROM tasks WHERE id = 't1'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(subject, "valid task");
        assert_eq!(description, "desc");
        assert_eq!(status, "pending");
        assert_eq!(active_form.as_deref(), Some("doing"));
        assert_eq!(blocked_by, "[]");
        assert_eq!(created_at, 1);
        assert_eq!(updated_at, 2);

        let input_tokens: i64 = db
            .conn
            .query_row(
                "SELECT input_tokens FROM cost_summaries WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            input_tokens, 42,
            "valid cost_summaries row survives with columns intact"
        );

        // FK now enforced: inserting a task with a bogus session_id errors.
        let err = db.conn.execute(
            "INSERT INTO tasks (id, session_id, subject, created_at, updated_at) VALUES ('t2', 'bogus', 'x', 0, 0)",
            [],
        );
        assert!(
            err.is_err(),
            "FK must reject a task pointing at a nonexistent session"
        );
    }

    #[test]
    fn fresh_db_migrates_to_latest_version_with_022_schema() {
        let db = in_memory();
        // Highest migration file as of writing is 032 (see `migrate()`'s array,
        // a local not reachable from here to derive this count automatically).
        let version: i64 = db
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 32);

        let has_transcripts: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_transcripts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_transcripts, 1);

        let fk_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('session_summaries') WHERE `table` = 'sessions'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            fk_count, 1,
            "session_summaries must carry the FK to sessions (022 rebuild)"
        );
    }

    #[test]
    fn apply_migration_rolls_back_on_error_and_is_retryable() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")
            .unwrap();

        // Second statement errors (duplicate table) — the transaction must
        // discard the first statement's effect along with the version bump.
        let bad_sql = "CREATE TABLE marker (x INTEGER); CREATE TABLE marker (x INTEGER);";
        assert!(Database::apply_migration(&conn, 1, bad_sql).is_err());

        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='marker'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_exists, 0,
            "first statement must not persist on rollback"
        );

        let version_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_version WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            version_count, 0,
            "schema_version must not be bumped on rollback"
        );

        // The corrected migration re-applies cleanly.
        let good_sql = "CREATE TABLE marker (x INTEGER);";
        Database::apply_migration(&conn, 1, good_sql).unwrap();

        let table_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='marker'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(table_exists, 1);

        let version_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_version WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version_count, 1);
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
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
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
    fn workspace_plans_dir_override() {
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp/repo").unwrap();
        // Default: none.
        assert!(db.get_workspace_plans_dir("ws1").unwrap().is_none());
        // Set + read back.
        db.set_workspace_plans_dir("ws1", Some("/plans/ws1"))
            .unwrap();
        assert_eq!(
            db.get_workspace_plans_dir("ws1").unwrap().as_deref(),
            Some("/plans/ws1")
        );
        // Empty string clears to default.
        db.set_workspace_plans_dir("ws1", Some("  ")).unwrap();
        assert!(db.get_workspace_plans_dir("ws1").unwrap().is_none());
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
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
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
            active_linear_issue_id: Some("iss-42".into()),
            pinned_linear_issue_ids: vec!["iss-7".into(), "iss-99".into()],
        };
        db.create_session(&session).unwrap();
        let loaded = db.find_session("s-cu").unwrap().unwrap();
        assert_eq!(loaded.active_clickup_task_id.as_deref(), Some("task-42"));
        assert_eq!(
            loaded.pinned_clickup_task_ids,
            vec!["task-7".to_string(), "task-99".to_string()]
        );
        assert_eq!(loaded.active_linear_issue_id.as_deref(), Some("iss-42"));
        assert_eq!(
            loaded.pinned_linear_issue_ids,
            vec!["iss-7".to_string(), "iss-99".to_string()]
        );
    }

    #[test]
    fn parse_pinned_linear_issue_ids_handles_null_and_garbage() {
        assert!(parse_pinned_linear_issue_ids(None).is_empty());
        assert!(parse_pinned_linear_issue_ids(Some("".into())).is_empty());
        assert!(parse_pinned_linear_issue_ids(Some("not json".into())).is_empty());
        assert_eq!(
            parse_pinned_linear_issue_ids(Some(r#"["i1","i2"]"#.into())),
            vec!["i1".to_string(), "i2".to_string()]
        );
    }

    #[test]
    fn linear_issue_binding_helpers_roundtrip() {
        let db = in_memory();
        db.create_workspace("ws1", "ws", "/tmp").unwrap();
        let session = Session {
            id: "s-li".into(),
            name: "li".into(),
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
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
        };
        db.create_session(&session).unwrap();

        db.set_active_linear_issue("s-li", Some("ENG-1")).unwrap();
        assert_eq!(
            db.find_session("s-li")
                .unwrap()
                .unwrap()
                .active_linear_issue_id
                .as_deref(),
            Some("ENG-1")
        );
        db.set_active_linear_issue("s-li", None).unwrap();
        assert!(
            db.find_session("s-li")
                .unwrap()
                .unwrap()
                .active_linear_issue_id
                .is_none()
        );

        db.add_pinned_linear_issue("s-li", "ENG-2").unwrap();
        db.add_pinned_linear_issue("s-li", "ENG-3").unwrap();
        db.add_pinned_linear_issue("s-li", "ENG-2").unwrap(); // idempotent
        assert_eq!(
            db.get_pinned_linear_issues("s-li").unwrap(),
            vec!["ENG-2".to_string(), "ENG-3".to_string()]
        );
        db.remove_pinned_linear_issue("s-li", "ENG-2").unwrap();
        assert_eq!(
            db.get_pinned_linear_issues("s-li").unwrap(),
            vec!["ENG-3".to_string()]
        );
    }
}
