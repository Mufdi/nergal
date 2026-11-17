use super::conversation::{
    ResumeCandidates, is_valid_conversation_id, pick_resume_id, project_dir_name, projects_dirs,
    resume_candidates,
};
use crate::db::Database;
use crate::models::{Session, SessionStatus};
use std::path::{Path, PathBuf};

fn seed_workspace(db: &Database, id: &str, repo: &str) {
    db.create_workspace(id, "ws", repo).unwrap();
}

fn seed_session(
    db: &Database,
    id: &str,
    workspace_id: &str,
    created_at: u64,
    internal_id: Option<&str>,
    transcript: Option<&str>,
) {
    let s = Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: workspace_id.to_string(),
        worktree_path: None,
        worktree_branch: None,
        merge_target: None,
        status: SessionStatus::Idle,
        created_at,
        updated_at: created_at,
        agent_id: "claude-code".into(),
        agent_internal_session_id: internal_id.map(String::from),
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
    if let Some(path) = transcript {
        db.set_session_transcript(id, path, created_at).unwrap();
    }
}

fn touch(projects: &Path, project: &str, id: &str) -> PathBuf {
    let dir = projects.join(project);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{id}.jsonl"));
    std::fs::write(&file, "").unwrap();
    file
}

fn candidates(
    stored_id: Option<&str>,
    transcript_path: Option<PathBuf>,
    claimed_by_older: bool,
    is_oldest_in_workspace: bool,
) -> ResumeCandidates {
    ResumeCandidates {
        stored_id: stored_id.map(String::from),
        transcript_path,
        claimed_by_older,
        is_oldest_in_workspace,
        repo_root: PathBuf::from("/repo"),
    }
}

#[test]
fn conversation_ids_accept_uuids_and_reject_path_characters() {
    assert!(is_valid_conversation_id(
        "0b1c2d3e-4f50-6789-abcd-ef0123456789"
    ));
    for bad in ["", "../x", "a/b", "a b", "a.jsonl", "-x", "--resume"] {
        assert!(!is_valid_conversation_id(bad), "accepted {bad:?}");
    }
}

#[test]
fn project_dir_name_replaces_every_non_alphanumeric_char() {
    assert_eq!(
        project_dir_name(Path::new("/home/u/00Custom Mods/X.y")),
        "-home-u-00Custom-Mods-X-y"
    );
}

#[test]
fn projects_dirs_includes_the_transcript_grandparent() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("custom").join("projects");
    let transcript = touch(&projects, "-repo", "aaaa-1111");

    let dirs = projects_dirs(Some(&transcript));

    assert!(dirs.contains(&projects), "dirs: {dirs:?}");
}

#[test]
fn resume_candidates_is_none_for_an_unknown_session() {
    let db = Database::open_in_memory().unwrap();
    assert!(resume_candidates(&db, "missing").is_none());
}

#[test]
fn resume_candidates_reports_the_session_fields() {
    let db = Database::open_in_memory().unwrap();
    seed_workspace(&db, "w", "/repo");
    seed_session(
        &db,
        "only",
        "w",
        100,
        Some("aaaa-1111"),
        Some("/p/-repo/bbbb-2222.jsonl"),
    );

    let c = resume_candidates(&db, "only").unwrap();

    assert_eq!(c.stored_id.as_deref(), Some("aaaa-1111"));
    assert_eq!(
        c.transcript_path,
        Some(PathBuf::from("/p/-repo/bbbb-2222.jsonl"))
    );
    assert_eq!(c.repo_root, PathBuf::from("/repo"));
    assert!(!c.claimed_by_older);
    assert!(c.is_oldest_in_workspace);
}

#[test]
fn a_shared_transcript_is_claimed_by_the_older_session_only() {
    let db = Database::open_in_memory().unwrap();
    seed_workspace(&db, "w", "/repo");
    let shared = Some("/p/-repo/cccc-3333.jsonl");
    seed_session(&db, "root", "w", 100, None, shared);
    seed_session(&db, "extra", "w", 200, None, shared);

    let root = resume_candidates(&db, "root").unwrap();
    let extra = resume_candidates(&db, "extra").unwrap();

    assert!(!root.claimed_by_older);
    assert!(root.is_oldest_in_workspace);
    assert!(extra.claimed_by_older);
    assert!(!extra.is_oldest_in_workspace);
}

#[test]
fn an_older_session_whose_id_equals_the_transcript_stem_claims_it() {
    let db = Database::open_in_memory().unwrap();
    seed_workspace(&db, "w", "/repo");
    seed_session(&db, "root", "w", 100, Some("dddd-4444"), None);
    seed_session(
        &db,
        "extra",
        "w",
        200,
        None,
        Some("/p/-repo/dddd-4444.jsonl"),
    );

    assert!(resume_candidates(&db, "extra").unwrap().claimed_by_older);
}

#[test]
fn a_transcript_in_another_workspace_is_not_claimed() {
    let db = Database::open_in_memory().unwrap();
    seed_workspace(&db, "w1", "/repo");
    seed_workspace(&db, "w2", "/other");
    let shared = Some("/p/-repo/eeee-5555.jsonl");
    seed_session(&db, "a", "w1", 100, None, shared);
    seed_session(&db, "b", "w2", 200, None, shared);

    assert!(!resume_candidates(&db, "b").unwrap().claimed_by_older);
}

#[test]
fn stored_id_with_a_transcript_on_disk_is_picked() {
    let root = tempfile::tempdir().unwrap();
    touch(root.path(), "-repo", "aaaa-1111");
    let c = candidates(Some("aaaa-1111"), None, false, true);

    assert_eq!(
        pick_resume_id(&c, &[root.path().to_path_buf()]).as_deref(),
        Some("aaaa-1111")
    );
}

#[test]
fn stored_id_without_a_file_starts_fresh_even_when_the_old_transcript_exists() {
    let root = tempfile::tempdir().unwrap();
    let old = touch(root.path(), "-repo", "aaaa-1111");
    let c = candidates(Some("bbbb-2222"), Some(old), false, true);

    assert_eq!(pick_resume_id(&c, &[root.path().to_path_buf()]), None);
}

#[test]
fn legacy_session_falls_back_to_its_transcript_stem() {
    let root = tempfile::tempdir().unwrap();
    let t = touch(root.path(), "-repo--worktrees-x", "bbbb-2222");
    let c = candidates(None, Some(t), false, false);

    assert_eq!(
        pick_resume_id(&c, &[root.path().to_path_buf()]).as_deref(),
        Some("bbbb-2222")
    );
}

#[test]
fn legacy_stem_whose_file_is_gone_starts_fresh() {
    let root = tempfile::tempdir().unwrap();
    let c = candidates(
        None,
        Some(root.path().join("-repo-x").join("ffff-6666.jsonl")),
        false,
        false,
    );

    assert_eq!(pick_resume_id(&c, &[root.path().to_path_buf()]), None);
}

#[test]
fn legacy_transcript_claimed_by_an_older_session_starts_fresh() {
    let root = tempfile::tempdir().unwrap();
    let t = touch(root.path(), "-repo--worktrees-x", "cccc-3333");
    let c = candidates(None, Some(t), true, false);

    assert_eq!(pick_resume_id(&c, &[root.path().to_path_buf()]), None);
}

#[test]
fn legacy_repo_root_transcript_stays_with_the_oldest_session() {
    let root = tempfile::tempdir().unwrap();
    let t = touch(root.path(), "-repo", "m1");
    let dirs = [root.path().to_path_buf()];

    let demoted = candidates(None, Some(t.clone()), false, false);
    let oldest = candidates(None, Some(t), false, true);

    assert_eq!(pick_resume_id(&demoted, &dirs), None);
    assert_eq!(pick_resume_id(&oldest, &dirs).as_deref(), Some("m1"));
}

#[test]
fn stored_id_with_path_characters_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    touch(root.path(), "-repo", "x");
    let c = candidates(Some("../x"), None, false, true);

    assert_eq!(pick_resume_id(&c, &[root.path().to_path_buf()]), None);
}
