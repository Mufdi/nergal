use super::*;
use crate::db::{Database, SharedDb};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn run_git(args: &[&str], cwd: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
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
        run_git(&["commit", "-q", "--allow-empty", "-m", "init"], &repo);
        run_git(
            &["remote", "add", "origin", origin.to_str().unwrap()],
            &repo,
        );
        run_git(&["push", "-q", "-u", "origin", "main"], &repo);
        let db = Database::open_in_memory().unwrap();
        db.create_workspace("w", "ws", repo.to_str().unwrap())
            .unwrap();
        Fixture {
            tmp,
            repo,
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
        std::process::Command::new("git")
            .args(["show-ref", "--verify", "--quiet"])
            .arg(format!("refs/heads/{branch}"))
            .current_dir(&self.repo)
            .status()
            .unwrap()
            .success()
    }

    fn add_worktree(&self, branch: &str, name: &str) -> PathBuf {
        let path = self.root().join("wt").join(name);
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

    fn registrations(&self) -> String {
        self.git(&["worktree", "list", "--porcelain"])
    }

    fn record(&self) -> PathBuf {
        self.root().join("data").join("deleted-branches.log")
    }
}

#[test]
fn a_stale_expected_sha_leaves_the_missing_worktree_registration_intact() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/y", "y");
    run_git(&["commit", "-q", "--allow-empty", "-m", "work"], &wt);
    std::fs::remove_dir_all(&wt).unwrap();
    let wrong = fx.branch_sha("main");
    assert_ne!(wrong, fx.branch_sha("nergal/y"));

    let result = delete_local_branch_core(&fx.db, "w", "nergal/y", &wrong, &fx.record()).unwrap();

    assert!(
        matches!(result, LocalBranchDelete::Kept { .. }),
        "got {result:?}"
    );
    assert!(fx.exists("nergal/y"));
    assert!(
        fx.registrations().contains(wt.to_str().unwrap()),
        "registration was cleared: {}",
        fx.registrations()
    );
}

#[test]
fn a_locked_missing_worktree_keeps_its_branch_and_registration() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/g", "g");
    fx.git(&["worktree", "lock", wt.to_str().unwrap()]);
    std::fs::remove_dir_all(&wt).unwrap();
    let sha = fx.branch_sha("nergal/g");

    let result = delete_local_branch_core(&fx.db, "w", "nergal/g", &sha, &fx.record()).unwrap();

    match result {
        LocalBranchDelete::Kept { message } => assert!(
            message.contains("it is checked out, or being rebased or bisected"),
            "message was: {message}"
        ),
        other => panic!("expected Kept, got {other:?}"),
    }
    assert!(fx.exists("nergal/g"));
    assert!(fx.registrations().contains(wt.to_str().unwrap()));
}

#[test]
fn an_unwritable_restore_record_keeps_the_branch_and_names_the_record_path() {
    let fx = Fixture::new();
    fx.git(&["branch", "nergal/r"]);
    let sha = fx.branch_sha("nergal/r");
    let blocker = fx.root().join("file");
    std::fs::write(&blocker, "not a dir").unwrap();
    let record = blocker.join("deleted-branches.log");

    let result = delete_local_branch_core(&fx.db, "w", "nergal/r", &sha, &record).unwrap();

    match result {
        LocalBranchDelete::Kept { message } => assert!(
            message.contains(record.to_str().unwrap()),
            "message was: {message}"
        ),
        other => panic!("expected Kept, got {other:?}"),
    }
    assert!(fx.exists("nergal/r"));
}
