use super::*;
use std::os::unix::fs::PermissionsExt;

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
    tmp: tempfile::TempDir,
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
        Fixture { tmp, repo, record }
    }

    fn branch_with_commit(&self, name: &str, push: bool) -> String {
        run_git(&["checkout", "-q", "-b", name], &self.repo);
        run_git(&["commit", "-q", "--allow-empty", "-m", "work"], &self.repo);
        if push {
            run_git(&["push", "-q", "-u", "origin", name], &self.repo);
        }
        let sha = self.sha(name);
        run_git(&["checkout", "-q", "main"], &self.repo);
        sha
    }

    fn sha(&self, branch: &str) -> String {
        run_git(&["rev-parse", &format!("refs/heads/{branch}")], &self.repo)
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

    fn drop(&self, branch: &str, target: Option<&str>) -> BranchOutcome {
        drop_session_branch(&self.repo, branch, target, &self.record)
    }
}

#[test]
fn a_pushed_branch_is_deleted_and_its_sha_returned() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", true);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(outcome, BranchOutcome::Deleted { sha });
    assert!(!fx.exists("nergal/x"));
}

#[test]
fn a_branch_merged_into_the_merge_target_is_deleted() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", false);
    run_git(&["merge", "-q", "--ff-only", "nergal/x"], &fx.repo);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(outcome, BranchOutcome::Deleted { sha });
    assert!(!fx.exists("nergal/x"));
}

#[test]
fn without_a_merge_target_the_repo_root_head_decides() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", false);
    run_git(&["merge", "-q", "--ff-only", "nergal/x"], &fx.repo);

    assert_eq!(fx.drop("nergal/x", None), BranchOutcome::Deleted { sha });
}

#[test]
fn a_detached_repo_root_head_containing_the_commits_counts_as_merged() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", false);
    run_git(&["merge", "-q", "--ff-only", "nergal/x"], &fx.repo);
    run_git(&["checkout", "-q", "--detach"], &fx.repo);

    assert_eq!(fx.drop("nergal/x", None), BranchOutcome::Deleted { sha });
}

#[test]
fn a_branch_with_a_local_only_commit_is_kept_and_leaves_no_record() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", false);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Unshared
        }
    );
    assert!(fx.exists("nergal/x"));
    assert!(!fx.record.exists());
}

#[test]
fn an_unknown_merge_target_keeps_the_branch() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);

    let outcome = fx.drop("nergal/x", Some("no-such-target"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Unshared
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_branch_checked_out_in_the_repo_root_is_kept_and_head_is_untouched() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    run_git(&["checkout", "-q", "nergal/x"], &fx.repo);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::InUse
        }
    );
    assert_eq!(
        run_git(&["symbolic-ref", "HEAD"], &fx.repo),
        "refs/heads/nergal/x"
    );
}

#[test]
fn a_branch_checked_out_in_a_linked_worktree_is_kept() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    let wt = fx.tmp.path().join("wt");
    run_git(
        &["worktree", "add", "-q", wt.to_str().unwrap(), "nergal/x"],
        &fx.repo,
    );

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::InUse
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn checked_out_matches_the_exact_branch_name_not_a_prefix() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    fx.branch_with_commit("nergal/x-2", true);
    run_git(&["checkout", "-q", "nergal/x-2"], &fx.repo);

    assert!(!branch_checked_out(&fx.repo, "nergal/x"));
    assert!(branch_checked_out(&fx.repo, "nergal/x-2"));
}

#[test]
fn a_branch_being_rebased_is_kept() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    let rebase = fx.repo.join(".git").join("rebase-merge");
    std::fs::create_dir_all(&rebase).unwrap();
    std::fs::write(rebase.join("head-name"), "refs/heads/nergal/x\n").unwrap();

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::InUse
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_branch_being_bisected_is_kept() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    std::fs::write(fx.repo.join(".git").join("BISECT_START"), "nergal/x\n").unwrap();

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::InUse
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_record_path_whose_parent_is_missing_is_created_and_gets_one_line() {
    let mut fx = Fixture::new();
    fx.record = fx
        .tmp
        .path()
        .join("missing")
        .join("sub")
        .join("deleted-branches.log");
    let sha = fx.branch_with_commit("nergal/x", true);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(outcome, BranchOutcome::Deleted { sha: sha.clone() });
    let content = std::fs::read_to_string(&fx.record).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(fx.repo.to_str().unwrap()), "{}", lines[0]);
    assert!(lines[0].contains("nergal/x"), "{}", lines[0]);
    assert!(lines[0].contains(&sha), "{}", lines[0]);
}

#[test]
fn the_record_appends_and_never_truncates_earlier_lines() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.record.parent().unwrap()).unwrap();
    std::fs::write(&fx.record, "earlier line\n").unwrap();
    fx.branch_with_commit("nergal/x", true);

    fx.drop("nergal/x", Some("main"));

    let content = std::fs::read_to_string(&fx.record).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "earlier line");
    assert!(lines[1].contains("nergal/x"));
}

#[test]
fn an_unwritable_record_keeps_the_branch_intact() {
    let mut fx = Fixture::new();
    let blocker = fx.tmp.path().join("file");
    std::fs::write(&blocker, "not a dir").unwrap();
    fx.record = blocker.join("deleted-branches.log");
    fx.branch_with_commit("nergal/x", true);

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::RecordFailed {
                path: fx.record.clone()
            }
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_compare_and_delete_refused_by_git_keeps_the_branch_as_moved() {
    let fx = Fixture::new();
    fx.branch_with_commit("nergal/x", true);
    let hook = fx
        .repo
        .join(".git")
        .join("hooks")
        .join("reference-transaction");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        "#!/bin/sh\nif [ \"$1\" = \"prepared\" ]; then exit 1; fi\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    // A global core.hooksPath would otherwise bypass the repo's own hooks dir.
    run_git(
        &[
            "config",
            "core.hooksPath",
            hook.parent().unwrap().to_str().unwrap(),
        ],
        &fx.repo,
    );

    let outcome = fx.drop("nergal/x", Some("main"));

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Moved
        }
    );
    assert!(fx.exists("nergal/x"));
}

#[test]
fn a_missing_branch_reports_missing() {
    let fx = Fixture::new();

    assert_eq!(fx.drop("nergal/gone", Some("main")), BranchOutcome::Missing);
}

#[test]
fn branch_state_reports_exists_missing_and_unknown() {
    let fx = Fixture::new();
    let sha = fx.branch_with_commit("nergal/x", false);
    let not_a_repo = tempfile::tempdir().unwrap();

    assert_eq!(
        branch_state(&fx.repo, "nergal/x"),
        BranchState::Exists { sha }
    );
    assert_eq!(branch_state(&fx.repo, "nergal/gone"), BranchState::Missing);
    assert_eq!(
        branch_state(not_a_repo.path(), "nergal/x"),
        BranchState::Unknown
    );
}

#[test]
fn delete_branch_if_at_refuses_when_the_branch_moved_past_the_sha() {
    let fx = Fixture::new();
    let old = fx.branch_with_commit("nergal/x", false);
    run_git(&["checkout", "-q", "nergal/x"], &fx.repo);
    run_git(&["commit", "-q", "--allow-empty", "-m", "more"], &fx.repo);
    run_git(&["checkout", "-q", "main"], &fx.repo);

    assert!(!delete_branch_if_at(&fx.repo, "nergal/x", &old));
    assert!(fx.exists("nergal/x"));
    assert!(delete_branch_if_at(
        &fx.repo,
        "nergal/x",
        &fx.sha("nergal/x")
    ));
    assert!(!fx.exists("nergal/x"));
}

#[test]
fn a_directory_without_git_or_missing_is_pending_delete_and_one_with_git_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    let gutted = tmp.path().join("gutted");
    std::fs::create_dir_all(&gutted).unwrap();
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

    assert!(worktree_pending_delete(&gutted));
    assert!(worktree_pending_delete(&tmp.path().join("missing")));
    assert!(!worktree_pending_delete(&healthy));
}

#[test]
fn a_git_file_whose_registration_was_dropped_is_pending_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let dropped = tmp.path().join("dropped");
    std::fs::create_dir_all(&dropped).unwrap();
    std::fs::write(
        dropped.join(".git"),
        format!(
            "gitdir: {}\n",
            tmp.path().join("repo/.git/worktrees/x").display()
        ),
    )
    .unwrap();
    let no_head = tmp.path().join("no-head");
    let empty_admin = tmp.path().join("empty-admin");
    std::fs::create_dir_all(&no_head).unwrap();
    std::fs::create_dir_all(&empty_admin).unwrap();
    std::fs::write(
        no_head.join(".git"),
        format!("gitdir: {}\n", empty_admin.display()),
    )
    .unwrap();
    let unparsable = tmp.path().join("unparsable");
    std::fs::create_dir_all(&unparsable).unwrap();
    std::fs::write(unparsable.join(".git"), "not a gitdir line\n").unwrap();

    assert!(worktree_pending_delete(&dropped));
    assert!(worktree_pending_delete(&no_head));
    assert!(worktree_pending_delete(&unparsable));
}

#[test]
fn a_git_directory_or_a_relative_gitdir_that_exists_is_not_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let linked = tmp.path().join("linked");
    std::fs::create_dir_all(linked.join("admin")).unwrap();
    std::fs::write(linked.join("admin").join("HEAD"), "ref: refs/heads/x\n").unwrap();
    std::fs::write(linked.join(".git"), "gitdir: admin\n").unwrap();

    assert!(!worktree_pending_delete(&root));
    assert!(!worktree_pending_delete(&linked));
}
