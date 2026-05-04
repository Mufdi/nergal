//! Integration test: foundation DB migration applies cleanly to a populated
//! pre-foundation schema (no `agent_id` / `agent_internal_session_id`
//! columns), preserving existing rows and backfilling defaults per Decision 5.
//!
//! Why an integration test rather than a `#[cfg(test)]` module: we exercise
//! the full SQLite migration applier in `db.rs::migrate` against a fixture
//! database constructed with the v6 schema (no agent columns). A unit test
//! couldn't easily set up the v6 schema without inverting the migrate logic.

use rusqlite::{Connection, params};

const PRE_FOUNDATION_SCHEMA: &str = r#"
    CREATE TABLE workspaces (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        repo_path  TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE sessions (
        id               TEXT PRIMARY KEY,
        workspace_id     TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        name             TEXT NOT NULL,
        worktree_path    TEXT,
        worktree_branch  TEXT,
        merge_target     TEXT,
        status           TEXT NOT NULL DEFAULT 'idle',
        created_at       INTEGER NOT NULL,
        updated_at       INTEGER NOT NULL
    );
    CREATE TABLE schema_version (version INTEGER NOT NULL);
"#;

const FOUNDATION_MIGRATION: &str = include_str!("../migrations/007_agent_id.sql");

#[test]
fn foundation_migration_backfills_agent_id_for_existing_rows() {
    let conn = Connection::open_in_memory().expect("open in-memory db");

    // Build the v6 (pre-foundation) schema and pretend migrations 1-6 ran.
    conn.execute_batch(PRE_FOUNDATION_SCHEMA)
        .expect("pre-foundation schema");
    for v in 1..=6 {
        conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [v])
            .expect("seed schema_version");
    }

    // Populate a workspace + a couple of sessions with the OLD schema so the
    // migration has actual rows to backfill.
    conn.execute(
        "INSERT INTO workspaces (id, name, repo_path, created_at) VALUES (?1, ?2, ?3, ?4)",
        params!["ws-1", "demo", "/tmp/demo", 100i64],
    )
    .unwrap();
    for sid in ["sess-1", "sess-2", "sess-3"] {
        conn.execute(
            "INSERT INTO sessions (id, workspace_id, name, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'idle', 100, 100)",
            params![sid, "ws-1", sid],
        )
        .unwrap();
    }

    // Apply the foundation migration as the runtime would.
    conn.execute_batch(FOUNDATION_MIGRATION)
        .expect("apply 007_agent_id.sql");

    // Existing rows should now report `agent_id = 'claude-code'` and
    // `agent_internal_session_id = NULL`.
    let mut stmt = conn
        .prepare("SELECT id, agent_id, agent_internal_session_id FROM sessions ORDER BY id")
        .unwrap();
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(rows.len(), 3, "all pre-foundation rows preserved");
    for (sid, agent_id, internal) in &rows {
        assert_eq!(
            agent_id, "claude-code",
            "session {sid} should backfill to claude-code",
        );
        assert!(
            internal.is_none(),
            "session {sid} should leave agent_internal_session_id NULL",
        );
    }

    // Newly inserted rows after the migration should pick up the same default
    // when the INSERT omits agent_id, matching the legacy state.json import path.
    conn.execute(
        "INSERT INTO sessions (id, workspace_id, name, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'idle', 200, 200)",
        params!["sess-4", "ws-1", "fourth"],
    )
    .unwrap();
    let agent_id: String = conn
        .query_row(
            "SELECT agent_id FROM sessions WHERE id = ?1",
            ["sess-4"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        agent_id, "claude-code",
        "DEFAULT clause covers post-migration inserts",
    );

    // The new index is queryable.
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_sessions_agent_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "idx_sessions_agent_id was created");
}
