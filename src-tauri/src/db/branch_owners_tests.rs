use crate::db::Database;
use crate::models::{BranchOwner, Session, SessionStatus};

fn seed(
    db: &Database,
    id: &str,
    workspace: &str,
    created_at: u64,
    branch: Option<&str>,
    merge_target: Option<&str>,
) {
    let s = Session {
        id: id.to_string(),
        name: format!("name-{id}"),
        workspace_id: workspace.to_string(),
        worktree_path: branch.map(|_| std::path::PathBuf::from(format!("/repo/.worktrees/{id}"))),
        worktree_branch: branch.map(str::to_string),
        merge_target: merge_target.map(str::to_string),
        status: SessionStatus::Idle,
        created_at,
        updated_at: created_at,
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
    db.create_workspace("other", "other", "/other").unwrap();
    db
}

#[test]
fn branch_owners_come_back_in_creation_order_with_name_and_merge_target() {
    let db = db();
    seed(&db, "late", "w", 200, Some("nergal/b"), None);
    seed(&db, "early", "w", 100, Some("nergal/a"), Some("develop"));

    let owners = db.branch_owners("w").unwrap();

    assert_eq!(
        owners,
        vec![
            BranchOwner {
                session_id: "early".into(),
                session_name: "name-early".into(),
                branch: "nergal/a".into(),
                merge_target: Some("develop".into()),
            },
            BranchOwner {
                session_id: "late".into(),
                session_name: "name-late".into(),
                branch: "nergal/b".into(),
                merge_target: None,
            },
        ]
    );
}

#[test]
fn branch_owners_exclude_repo_root_sessions_and_other_workspaces() {
    let db = db();
    seed(&db, "root", "w", 50, None, None);
    seed(&db, "mine", "w", 100, Some("nergal/a"), None);
    seed(&db, "theirs", "other", 100, Some("nergal/z"), None);

    let owners = db.branch_owners("w").unwrap();

    let ids: Vec<&str> = owners.iter().map(|o| o.session_id.as_str()).collect();
    assert_eq!(ids, vec!["mine"]);
}
