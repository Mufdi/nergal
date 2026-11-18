use super::*;
use crate::models::{Session, SessionStatus};

fn seed(db: &Database, id: &str, workspace_id: &str, worktree: Option<&str>) {
    let s = Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: workspace_id.to_string(),
        worktree_path: worktree.map(PathBuf::from),
        worktree_branch: worktree.map(|_| "feat/x".to_string()),
        merge_target: None,
        status: SessionStatus::Idle,
        created_at: 100,
        updated_at: 100,
        agent_id: "claude-code".into(),
        agent_internal_session_id: None,
        agent_capabilities: Vec::new(),
        worktree_pending_delete: false,
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

fn db_with_workspaces() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.create_workspace("w", "ws", "/repo").unwrap();
    db.create_workspace("other", "other", "/other").unwrap();
    db
}

#[test]
fn the_only_root_session_next_to_a_worktree_session_is_the_last_root() {
    let db = db_with_workspaces();
    seed(&db, "r1", "w", None);
    seed(&db, "t1", "w", Some("/repo/.worktrees/t1"));

    assert!(db.is_last_root_session("r1").unwrap());
}

#[test]
fn one_of_two_root_sessions_is_not_the_last_root() {
    let db = db_with_workspaces();
    seed(&db, "r1", "w", None);
    seed(&db, "r2", "w", None);

    assert!(!db.is_last_root_session("r2").unwrap());
}

#[test]
fn a_worktree_session_is_never_the_last_root() {
    let db = db_with_workspaces();
    seed(&db, "r1", "w", None);
    seed(&db, "t1", "w", Some("/repo/.worktrees/t1"));

    assert!(!db.is_last_root_session("t1").unwrap());
}

#[test]
fn a_root_session_in_another_workspace_does_not_count() {
    let db = db_with_workspaces();
    seed(&db, "r1", "w", None);
    seed(&db, "r2", "other", None);

    assert!(db.is_last_root_session("r1").unwrap());
}

#[test]
fn an_unknown_session_is_not_the_last_root() {
    let db = db_with_workspaces();

    assert!(!db.is_last_root_session("missing").unwrap());
}
