use super::*;

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

fn branch_exists(repo: &Path, full_ref: &str) -> bool {
    std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet", full_ref])
        .current_dir(repo)
        .status()
        .unwrap()
        .success()
}

fn repo_with_symref() -> (tempfile::TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&["init", "-q", "-b", "main"], &repo);
    run_git(&["commit", "-q", "--allow-empty", "-m", "init"], &repo);
    run_git(
        &["symbolic-ref", "refs/heads/nergal/x", "refs/heads/main"],
        &repo,
    );
    let main_sha = run_git(&["rev-parse", "refs/heads/main"], &repo);
    (tmp, repo, main_sha)
}

#[test]
fn a_symref_session_branch_is_kept_and_its_target_survives() {
    let (tmp, repo, main_sha) = repo_with_symref();
    let record = tmp.path().join("data").join("deleted-branches.log");

    let outcome = drop_session_branch(&repo, "nergal/x", None, &record);

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Unchecked
        }
    );
    assert!(branch_exists(&repo, "refs/heads/main"));
    assert_eq!(run_git(&["rev-parse", "HEAD"], &repo), main_sha);
    assert!(!record.exists());
}

#[test]
fn compare_and_delete_never_follows_a_symref() {
    let (_tmp, repo, main_sha) = repo_with_symref();

    delete_branch_if_at(&repo, "nergal/x", &main_sha);

    assert!(branch_exists(&repo, "refs/heads/main"));
}

#[test]
fn shell_quote_keeps_spaces_and_single_quotes_in_one_word() {
    assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
    assert_eq!(shell_quote("/it's"), r"'/it'\''s'");
}

#[test]
fn permission_remedy_quotes_the_worktree_path() {
    let err = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    let path = Path::new("/home/u/dev projects/app/.worktrees/nergal/x");

    let remedy = permission_remedy(&err, path);

    assert!(
        remedy.contains("rm -rf '/home/u/dev projects/app/.worktrees/nergal/x'"),
        "{remedy}"
    );
}
