use super::refuse_last_root_session;
use crate::db::Database;
use crate::models::{Session, SessionStatus};

fn seed(db: &Database, id: &str, worktree: Option<&str>) {
    let s = Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: "w".into(),
        worktree_path: worktree.map(std::path::PathBuf::from),
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

fn db() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.create_workspace("w", "ws", "/repo").unwrap();
    db
}

#[test]
fn the_only_repo_root_session_is_refused_and_its_row_stays() {
    let db = db();
    seed(&db, "r1", None);
    seed(&db, "t1", Some("/repo/.worktrees/t1"));

    let err = refuse_last_root_session(&db, "r1").unwrap_err();

    assert_eq!(
        err,
        "The last repo-root session of a workspace cannot be deleted"
    );
    assert!(db.find_session("r1").unwrap().is_some());
}

#[test]
fn one_of_two_repo_root_sessions_is_accepted() {
    let db = db();
    seed(&db, "r1", None);
    seed(&db, "r2", None);

    assert_eq!(refuse_last_root_session(&db, "r2"), Ok(()));
}
