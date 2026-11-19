use super::*;
use crate::db::{Database, SharedDb};
use crate::models::{BranchOwner, Session, SessionStatus};
use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn git_env(args: &[&str], cwd: &Path, date: Option<i64>) -> std::process::Output {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd);
    if let Some(unix) = date {
        let stamp = format!("@{unix} +0000");
        cmd.env("GIT_AUTHOR_DATE", &stamp)
            .env("GIT_COMMITTER_DATE", &stamp);
    }
    cmd.output().unwrap()
}

fn run_git(args: &[&str], cwd: &Path) -> String {
    let out = git_env(args, cwd, None);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_git_dated(args: &[&str], cwd: &Path, unix: i64) -> String {
    let out = git_env(args, cwd, Some(unix));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Fixture {
    tmp: tempfile::TempDir,
    repo: PathBuf,
    record: PathBuf,
    db: SharedDb,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        let origin = root.join("origin.git");
        let hooks = root.join("hooks");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&origin).unwrap();
        std::fs::create_dir_all(&hooks).unwrap();
        run_git(&["init", "-q", "--bare", "-b", "main"], &origin);
        run_git(&["init", "-q", "-b", "main"], &repo);
        run_git(
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
            &repo,
        );
        run_git_dated(&["commit", "-q", "--allow-empty", "-m", "init"], &repo, 500);
        run_git(
            &["remote", "add", "origin", origin.to_str().unwrap()],
            &repo,
        );
        run_git(&["push", "-q", "-u", "origin", "main"], &repo);
        let record = root.join("data").join("deleted-branches.log");
        let db = Database::open_in_memory().unwrap();
        db.create_workspace("w", "ws", repo.to_str().unwrap())
            .unwrap();
        Fixture {
            tmp,
            repo,
            record,
            db: Arc::new(Mutex::new(db)),
        }
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().canonicalize().unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(args, &self.repo)
    }

    fn branch_sha(&self, branch: &str) -> String {
        self.git(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    fn exists(&self, branch: &str) -> bool {
        git_env(
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            &self.repo,
            None,
        )
        .status
        .success()
    }

    /// Branch off main whose commits are dated `base_unix + i`.
    fn local_branch(&self, name: &str, commits: &[&str], base_unix: i64) -> String {
        self.git(&["checkout", "-q", "-b", name, "main"]);
        for (i, subject) in commits.iter().enumerate() {
            run_git_dated(
                &["commit", "-q", "--allow-empty", "-m", subject],
                &self.repo,
                base_unix + i as i64,
            );
        }
        let sha = self.branch_sha(name);
        self.git(&["checkout", "-q", "main"]);
        sha
    }

    fn worktree_path(&self, name: &str) -> PathBuf {
        self.root().join("wt").join(name)
    }

    fn add_worktree(&self, branch: &str, name: &str) -> PathBuf {
        let path = self.worktree_path(name);
        self.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            path.to_str().unwrap(),
        ]);
        path
    }

    fn worktree_list(&self) -> String {
        self.git(&["worktree", "list", "--porcelain"])
    }

    fn seed_owner(&self, id: &str, name: &str, branch: &str, merge_target: Option<&str>) {
        let s = Session {
            id: id.to_string(),
            name: name.to_string(),
            workspace_id: "w".into(),
            worktree_path: Some(PathBuf::from(format!("/nowhere/{id}"))),
            worktree_branch: Some(branch.to_string()),
            merge_target: merge_target.map(str::to_string),
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
        self.db.lock().unwrap().create_session(&s).unwrap();
    }

    fn owner(session_id: &str, session_name: &str, branch: &str) -> BranchOwner {
        BranchOwner {
            session_id: session_id.into(),
            session_name: session_name.into(),
            branch: branch.into(),
            merge_target: None,
        }
    }

    fn delete(&self, branch: &str, expected_sha: &str) -> Result<LocalBranchDelete, String> {
        delete_local_branch_core(&self.db, "w", branch, expected_sha, &self.record)
    }

    fn open(&self, branch: &str) -> Result<(Session, PathBuf), String> {
        open_branch_session_core(&self.db, "w", branch, 1700, Vec::new())
    }

    fn session_exists(&self, id: &str) -> bool {
        self.db.lock().unwrap().find_session(id).unwrap().is_some()
    }

    fn slug_dir(&self, slug: &str) -> PathBuf {
        self.repo.join(".worktrees/nergal").join(slug)
    }
}

fn row_names(rows: &[LocalBranchRow]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn a_local_only_branch_row_counts_its_pending_commits_and_has_no_session() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one", "two"], 1000);

    let rows = local_branch_rows(&fx.repo, &[]).unwrap();

    assert_eq!(
        rows,
        vec![LocalBranchRow {
            name: "nergal/a".into(),
            sha,
            pending: Some(2),
            last_commit_unix: 1001,
            last_subject: "two".into(),
            session: None,
            in_use: false,
            missing_worktree: None,
        }]
    );
}

#[test]
fn an_owned_branch_row_carries_its_session() {
    let fx = Fixture::new();
    fx.local_branch("nergal/a", &["one"], 1000);
    let owners = [Fixture::owner("s1", "my session", "nergal/a")];

    let rows = local_branch_rows(&fx.repo, &owners).unwrap();

    assert_eq!(
        rows[0].session,
        Some(BranchSession {
            id: "s1".into(),
            name: "my session".into()
        })
    );
}

#[test]
fn a_branch_checked_out_in_a_linked_worktree_is_in_use() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/w", "w");
    run_git_dated(&["commit", "-q", "--allow-empty", "-m", "work"], &wt, 1000);

    let rows = local_branch_rows(&fx.repo, &[]).unwrap();

    assert!(rows[0].in_use);
    assert_eq!(rows[0].missing_worktree, None);
}

#[test]
fn a_branch_whose_worktree_directory_is_gone_reports_the_missing_path() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/w", "w");
    run_git_dated(&["commit", "-q", "--allow-empty", "-m", "work"], &wt, 1000);
    std::fs::remove_dir_all(&wt).unwrap();

    let rows = local_branch_rows(&fx.repo, &[]).unwrap();

    assert!(!rows[0].in_use);
    assert_eq!(
        rows[0].missing_worktree,
        Some(wt.to_string_lossy().into_owned())
    );
}

#[test]
fn a_pushed_branch_is_absent_from_the_rows() {
    let fx = Fixture::new();
    fx.local_branch("nergal/pushed", &["one"], 1000);
    fx.git(&["push", "-q", "-u", "origin", "nergal/pushed"]);
    fx.local_branch("nergal/local", &["one"], 2000);

    let rows = local_branch_rows(&fx.repo, &[]).unwrap();

    assert_eq!(row_names(&rows), vec!["nergal/local"]);
}

#[test]
fn an_unknown_merge_target_leaves_the_pending_count_unknown() {
    let fx = Fixture::new();
    fx.local_branch("nergal/a", &["one"], 1000);
    let owners = [BranchOwner {
        merge_target: Some("gone".into()),
        ..Fixture::owner("s1", "n", "nergal/a")
    }];

    let rows = local_branch_rows(&fx.repo, &owners).unwrap();

    assert_eq!(rows[0].pending, None);
}

#[test]
fn a_directory_that_is_not_a_repo_gives_no_rows() {
    let tmp = tempfile::tempdir().unwrap();

    assert_eq!(local_branch_rows(tmp.path(), &[]), Ok(vec![]));
}

#[test]
fn deleting_with_an_invalid_name_is_kept_before_anything_is_looked_up() {
    let fx = Fixture::new();

    for name in ["feature/x", "nergal/../x", "main"] {
        let result = delete_local_branch_core(&fx.db, "no-such-ws", name, "abc1", &fx.record);

        match result {
            Ok(LocalBranchDelete::Kept { message }) => {
                assert!(
                    message.contains("not a Nergal branch name"),
                    "message for {name} was: {message}"
                );
            }
            other => panic!("expected Kept for {name}, got {other:?}"),
        }
    }
}

#[test]
fn deleting_a_branch_a_session_owns_is_kept_naming_the_session() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);
    fx.seed_owner("s1", "the owner", "nergal/a", None);

    let result = fx.delete("nergal/a", &sha).unwrap();

    match result {
        LocalBranchDelete::Kept { message } => assert!(message.contains("the owner")),
        other => panic!("expected Kept, got {other:?}"),
    }
    assert!(fx.exists("nergal/a"));
}

#[test]
fn deleting_in_an_unknown_workspace_is_an_error() {
    let fx = Fixture::new();

    let result = delete_local_branch_core(&fx.db, "no-such-ws", "nergal/a", "abc1", &fx.record);

    assert_eq!(result, Err("workspace not found".to_string()));
}

#[test]
fn deleting_a_local_only_branch_removes_the_ref_and_appends_the_restore_record() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);

    let result = fx.delete("nergal/a", &sha).unwrap();

    assert_eq!(
        result,
        LocalBranchDelete::Deleted {
            sha: sha.clone(),
            repo: fx.repo.display().to_string()
        }
    );
    assert!(!fx.exists("nergal/a"));
    let record = std::fs::read_to_string(&fx.record).unwrap();
    assert!(
        record.trim_end().ends_with(&format!("\tnergal/a\t{sha}")),
        "record was: {record}"
    );
}

#[test]
fn deleting_a_branch_held_by_a_removed_worktree_drops_its_registration_too() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/w", "w");
    run_git_dated(&["commit", "-q", "--allow-empty", "-m", "work"], &wt, 1000);
    let sha = fx.branch_sha("nergal/w");
    std::fs::remove_dir_all(&wt).unwrap();

    let result = fx.delete("nergal/w", &sha).unwrap();

    assert!(matches!(result, LocalBranchDelete::Deleted { .. }));
    assert!(!fx.exists("nergal/w"));
    assert!(!fx.worktree_list().contains(wt.to_str().unwrap()));
}

#[test]
fn deleting_a_branch_that_does_not_exist_reports_missing() {
    let fx = Fixture::new();

    assert_eq!(
        fx.delete("nergal/ghost", "abc1234"),
        Ok(LocalBranchDelete::Missing)
    );
}

#[test]
fn deleting_with_a_stale_sha_is_kept_and_the_branch_stays() {
    let fx = Fixture::new();
    fx.local_branch("nergal/a", &["one"], 1000);
    let stale = fx.branch_sha("main");

    let result = fx.delete("nergal/a", &stale).unwrap();

    match result {
        LocalBranchDelete::Kept { message } => assert!(message.contains("nergal/a")),
        other => panic!("expected Kept, got {other:?}"),
    }
    assert!(fx.exists("nergal/a"));
}

#[test]
fn deleting_a_branch_checked_out_in_a_live_worktree_is_kept() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/w", "w");
    run_git_dated(&["commit", "-q", "--allow-empty", "-m", "work"], &wt, 1000);
    let sha = fx.branch_sha("nergal/w");

    let result = fx.delete("nergal/w", &sha).unwrap();

    assert!(matches!(result, LocalBranchDelete::Kept { .. }));
    assert!(fx.exists("nergal/w"));
    assert!(wt.exists());
}

#[test]
fn opening_a_session_attaches_a_worktree_to_the_existing_branch() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/s", &["one"], 1000);

    let (session, repo) = fx.open("nergal/s").unwrap();

    let wt = fx.slug_dir("s");
    assert_eq!(repo, fx.repo);
    assert_eq!(session.name, "s");
    assert_eq!(session.workspace_id, "w");
    assert_eq!(session.worktree_path, Some(wt.clone()));
    assert_eq!(session.worktree_branch, Some("nergal/s".to_string()));
    assert_eq!(
        run_git(&["symbolic-ref", "HEAD"], &wt),
        "refs/heads/nergal/s"
    );
    assert_eq!(run_git(&["rev-parse", "HEAD"], &wt), sha);
    assert_eq!(fx.branch_sha("nergal/s"), sha);
}

#[test]
fn opening_a_session_inserts_its_row() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["one"], 1000);

    let (session, _) = fx.open("nergal/s").unwrap();

    assert!(fx.session_exists(&session.id));
}

#[test]
fn a_branch_whose_worktree_directory_was_removed_can_be_reopened() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/s", "s");
    run_git_dated(&["commit", "-q", "--allow-empty", "-m", "work"], &wt, 1000);
    let sha = fx.branch_sha("nergal/s");
    std::fs::remove_dir_all(&wt).unwrap();

    let (session, _) = fx.open("nergal/s").unwrap();

    let reopened = session.worktree_path.unwrap();
    assert_eq!(
        run_git(&["symbolic-ref", "HEAD"], &reopened),
        "refs/heads/nergal/s"
    );
    assert_eq!(run_git(&["rev-parse", "HEAD"], &reopened), sha);
}

#[test]
fn opening_a_branch_a_session_owns_is_refused_naming_it_and_creates_nothing() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["one"], 1000);
    fx.seed_owner("s1", "the owner", "nergal/s", None);

    let err = fx.open("nergal/s").unwrap_err();

    assert!(err.contains("the owner"), "error was: {err}");
    assert!(!fx.slug_dir("s").exists());
}

#[test]
fn opening_over_an_existing_directory_is_refused_naming_it_and_creates_nothing() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["one"], 1000);
    let dir = fx.slug_dir("s");
    std::fs::create_dir_all(&dir).unwrap();

    let err = fx.open("nergal/s").unwrap_err();

    assert!(err.contains(&dir.display().to_string()), "error was: {err}");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    assert!(!fx.session_exists("w-1700"));
}

#[test]
fn opening_a_branch_checked_out_elsewhere_is_refused_and_creates_nothing() {
    let fx = Fixture::new();
    fx.add_worktree("nergal/s", "elsewhere");

    let err = fx.open("nergal/s").unwrap_err();

    assert!(err.contains("checked out"), "error was: {err}");
    assert!(!fx.slug_dir("s").exists());
}

#[test]
fn opening_an_invalid_name_is_refused() {
    let fx = Fixture::new();

    assert!(fx.open("feature/x").is_err());
    assert!(fx.open("nergal/../x").is_err());
}

#[test]
fn opening_a_branch_that_does_not_exist_is_refused_and_never_creates_it() {
    let fx = Fixture::new();

    assert!(fx.open("nergal/ghost").is_err());
    assert!(!fx.exists("nergal/ghost"));
    assert!(!fx.slug_dir("ghost").exists());
}

fn unsaved_session(id: &str, branch: &str, worktree: &std::path::Path) -> Session {
    Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: "w".into(),
        worktree_path: Some(worktree.to_path_buf()),
        worktree_branch: Some(branch.to_string()),
        merge_target: None,
        status: SessionStatus::Idle,
        created_at: 200,
        updated_at: 200,
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
    }
}

#[test]
fn finishing_inserts_the_session_row() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["one"], 1000);
    let wt = crate::worktree::local_branches::add_worktree_for_branch(&fx.repo, "s").unwrap();
    let session = unsaved_session("fresh", "nergal/s", &wt);

    let finished = finish_branch_session(&fx.db, &fx.repo, &wt, session).unwrap();

    assert!(fx.session_exists(&finished.id));
    assert!(wt.exists());
}

#[test]
fn finishing_after_a_rival_took_the_branch_fails_naming_it_and_removes_the_worktree() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["one"], 1000);
    let wt = crate::worktree::local_branches::add_worktree_for_branch(&fx.repo, "s").unwrap();
    let session = unsaved_session("fresh", "nergal/s", &wt);
    let id = session.id.clone();
    fx.seed_owner("rival", "the rival", "nergal/s", None);

    let err = finish_branch_session(&fx.db, &fx.repo, &wt, session).unwrap_err();

    assert!(err.contains("the rival"), "error was: {err}");
    assert!(!wt.exists());
    assert!(!fx.worktree_list().contains(wt.to_str().unwrap()));
    assert!(!fx.session_exists(&id));
}

#[cfg(unix)]
#[test]
fn a_symlinked_alias_of_a_repo_shares_its_lock() {
    let fx = Fixture::new();
    let alias = fx.root().join("alias");
    std::os::unix::fs::symlink(&fx.repo, &alias).unwrap();

    assert!(Arc::ptr_eq(
        &branch_repo_lock(&fx.repo),
        &branch_repo_lock(&alias)
    ));
}

#[test]
fn different_repos_get_different_locks() {
    let a = Fixture::new();
    let b = Fixture::new();

    assert!(!Arc::ptr_eq(
        &branch_repo_lock(&a.repo),
        &branch_repo_lock(&b.repo)
    ));
}

#[test]
fn a_delete_waits_for_the_repo_lock_without_holding_the_db_guard() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);
    let lock = branch_repo_lock(&fx.repo);
    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let (tx, rx) = std::sync::mpsc::channel();
    let db = fx.db.clone();
    let record = fx.record.clone();
    let worker = std::thread::spawn(move || {
        let result = delete_local_branch_core(&db, "w", "nergal/a", &sha, &record);
        tx.send(result).unwrap();
    });

    let early = rx.recv_timeout(Duration::from_millis(300));
    let guard_free = fx.db.try_lock().is_ok();
    drop(guard);
    let late = rx.recv_timeout(Duration::from_secs(20));
    worker.join().unwrap();

    assert!(matches!(early, Err(RecvTimeoutError::Timeout)));
    assert!(guard_free, "the delete kept the DB guard while waiting");
    assert!(matches!(late, Ok(Ok(LocalBranchDelete::Deleted { .. }))));
    assert!(!fx.exists("nergal/a"));
}

#[test]
fn a_session_commit_scope_deserializes_from_its_camel_case_json() {
    let scope: CommitScope = serde_json::from_str(r#"{"kind":"session","sessionId":"s"}"#).unwrap();

    let CommitScope::Session { session_id } = scope else {
        panic!("expected the session variant");
    };
    assert_eq!(session_id, "s");
}

#[test]
fn a_workspace_commit_scope_deserializes_from_its_camel_case_json() {
    let scope: CommitScope =
        serde_json::from_str(r#"{"kind":"workspace","workspaceId":"w"}"#).unwrap();

    let CommitScope::Workspace { workspace_id } = scope else {
        panic!("expected the workspace variant");
    };
    assert_eq!(workspace_id, "w");
}
