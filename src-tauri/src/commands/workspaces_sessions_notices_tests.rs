use super::*;
use crate::commands::workspaces_sessions::refuse_launch;
use crate::worktree::{ReclaimFailure, ReclaimIssue};
use std::path::{Path, PathBuf};

fn run_git(args: &[&str], cwd: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
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
    _tmp: tempfile::TempDir,
    repo: PathBuf,
    record: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let origin = tmp.path().join("origin.git");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&origin).unwrap();
        run_git(&["init", "-q", "--bare", "-b", "main"], &origin);
        run_git(&["init", "-q", "-b", "main"], &repo);
        run_git(&["commit", "-q", "--allow-empty", "-m", "init"], &repo);
        run_git(
            &["remote", "add", "origin", origin.to_str().unwrap()],
            &repo,
        );
        run_git(&["push", "-q", "-u", "origin", "main"], &repo);
        let record = tmp.path().join("data").join("deleted-branches.log");
        Fixture {
            _tmp: tmp,
            repo,
            record,
        }
    }

    fn branch_with_commit(&self, name: &str, push: bool) -> String {
        run_git(&["checkout", "-q", "-b", name], &self.repo);
        run_git(&["commit", "-q", "--allow-empty", "-m", "work"], &self.repo);
        if push {
            run_git(&["push", "-q", "-u", "origin", name], &self.repo);
        }
        let sha = run_git(&["rev-parse", &format!("refs/heads/{name}")], &self.repo);
        run_git(&["checkout", "-q", "main"], &self.repo);
        sha
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

    fn notices(
        &self,
        removal: anyhow::Result<()>,
        branch: Option<&str>,
    ) -> Result<Vec<CleanupNotice>, String> {
        notices_after_removal(removal, &self.repo, branch, Some("main"), &self.record)
    }
}

#[test]
fn a_clean_removal_of_a_pushed_branch_yields_one_branch_deleted_notice_with_the_restore_command() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", true);

    let notices = fx.notices(Ok(()), Some("nergal/x")).unwrap();

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, "branch_deleted");
    assert!(
        notices[0]
            .message
            .contains(&format!("git branch nergal/x {sha}")),
        "{}",
        notices[0].message
    );
    assert!(!fx.exists("nergal/x"));
}

#[test]
fn a_clean_removal_of_a_local_only_branch_yields_one_branch_kept_notice() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", false);

    let notices = fx.notices(Ok(()), Some("nergal/x")).unwrap();

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, "branch_kept");
    assert!(notices[0].message.contains("nergal/x"));
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_clean_removal_without_a_branch_or_with_a_missing_one_yields_no_notice() {
    let fx = Fixture::new();

    assert_eq!(fx.notices(Ok(()), None).unwrap(), vec![]);
    assert_eq!(fx.notices(Ok(()), Some("nergal/gone")).unwrap(), vec![]);
}

#[test]
fn a_reclaim_failure_refuses_the_delete_and_leaves_the_branch() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    let err = anyhow::Error::new(ReclaimFailure {
        issue: ReclaimIssue::DockerFailed,
        message: "x".into(),
    });

    let result = fx.notices(Err(err), Some("nergal/x"));

    assert!(result.is_err());
    assert!(fx.exists("nergal/x"));
}

#[test]
fn another_removal_failure_reports_worktree_left_naming_the_kept_branch() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);

    let notices = fx
        .notices(Err(anyhow::anyhow!("disk on fire")), Some("nergal/x"))
        .unwrap();

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].kind, "worktree_left");
    assert!(notices[0].message.contains("disk on fire"));
    assert!(notices[0].message.contains("branch nergal/x kept"));
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_notice_serializes_to_kind_and_message() {
    let notice = CleanupNotice {
        kind: "branch_kept",
        message: "m".into(),
    };

    assert_eq!(
        serde_json::to_value(&notice).unwrap(),
        serde_json::json!({ "kind": "branch_kept", "message": "m" })
    );
}

#[test]
fn refuse_launch_rejects_a_partly_deleted_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let gutted = tmp.path().join("gutted");
    std::fs::create_dir_all(&gutted).unwrap();

    assert_eq!(
        refuse_launch(Some(&gutted), None),
        Err("This worktree was partly deleted; delete the session to finish".to_string())
    );
    assert!(refuse_launch(Some(&tmp.path().join("missing")), None).is_err());
}

#[test]
fn refuse_launch_allows_a_healthy_worktree_and_a_repo_root_session() {
    let tmp = tempfile::tempdir().unwrap();
    let healthy = tmp.path().join("healthy");
    std::fs::create_dir_all(&healthy).unwrap();
    let admin = tmp.path().join("admin");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join("HEAD"), "ref: refs/heads/x\n").unwrap();
    std::fs::write(
        healthy.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();

    assert_eq!(refuse_launch(Some(&healthy), None), Ok(()));
    assert_eq!(refuse_launch(Some(&healthy), Some(&healthy)), Ok(()));
    assert_eq!(refuse_launch(None, None), Ok(()));
}

#[test]
fn refuse_launch_rejects_a_cwd_that_does_not_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let healthy = tmp.path().join("healthy");
    std::fs::create_dir_all(&healthy).unwrap();
    let admin = tmp.path().join("admin");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join("HEAD"), "ref: refs/heads/x\n").unwrap();
    std::fs::write(
        healthy.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    let missing = healthy.join("sub");

    let err = refuse_launch(Some(&healthy), Some(&missing)).unwrap_err();

    assert_eq!(
        err,
        format!("Working directory {} does not exist", missing.display())
    );
}
