use super::reconcile_worktrees;
use crate::db::Database;
use crate::models::{Session, SessionStatus};
use std::path::PathBuf;

fn seed(
    db: &Database,
    id: &str,
    name: &str,
    created_at: u64,
    worktree: Option<PathBuf>,
    internal: Option<&str>,
) {
    let s = Session {
        id: id.to_string(),
        name: name.to_string(),
        workspace_id: "w".into(),
        worktree_branch: worktree.as_ref().map(|_| "feat/x".to_string()),
        worktree_path: worktree,
        merge_target: None,
        status: SessionStatus::Idle,
        created_at,
        updated_at: created_at,
        agent_id: "claude-code".into(),
        agent_internal_session_id: internal.map(String::from),
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

fn db_with_workspace() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.create_workspace("w", "ws", "/repo").unwrap();
    db
}

#[test]
fn missing_worktrees_become_repo_root_sessions_that_keep_their_conversation() {
    let root = tempfile::tempdir().unwrap();
    let present = root.path().join("present");
    std::fs::create_dir_all(&present).unwrap();
    let db = db_with_workspace();
    seed(
        &db,
        "late",
        "Zeta",
        300,
        Some(root.path().join("gone-late")),
        None,
    );
    seed(
        &db,
        "early",
        "Fixes",
        200,
        Some(root.path().join("gone-early")),
        Some("eeee-5555"),
    );
    seed(
        &db,
        "ok",
        "Present",
        250,
        Some(present.clone()),
        Some("ffff-6666"),
    );

    let demoted = reconcile_worktrees(&db);

    assert_eq!(demoted, vec!["Fixes".to_string(), "Zeta".to_string()]);
    let early = db.find_session("early").unwrap().unwrap();
    assert_eq!(early.worktree_path, None);
    assert_eq!(early.worktree_branch, None);
    assert_eq!(
        early.agent_internal_session_id.as_deref(),
        Some("eeee-5555")
    );
    let ok = db.find_session("ok").unwrap().unwrap();
    assert_eq!(ok.worktree_path, Some(present));
    assert_eq!(ok.worktree_branch.as_deref(), Some("feat/x"));
}

#[test]
fn present_worktrees_change_nothing_and_report_nothing() {
    let root = tempfile::tempdir().unwrap();
    let wt = root.path().join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let db = db_with_workspace();
    seed(&db, "a", "A", 100, Some(wt.clone()), Some("aaaa-1111"));
    seed(&db, "r", "Root", 50, None, None);

    assert_eq!(reconcile_worktrees(&db), Vec::<String>::new());

    let a = db.find_session("a").unwrap().unwrap();
    assert_eq!(a.worktree_path, Some(wt));
    assert_eq!(a.worktree_branch.as_deref(), Some("feat/x"));
}
