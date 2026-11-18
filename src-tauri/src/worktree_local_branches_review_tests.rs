use super::local_branches::{commit_file_diff, commit_files};
use std::path::{Path, PathBuf};

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

fn repo_with_non_ascii_commit() -> (tempfile::TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().canonicalize().unwrap().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&["init", "-q", "-b", "main"], &repo);
    run_git(&["commit", "-q", "--allow-empty", "-m", "init"], &repo);
    std::fs::write(repo.join("café.txt"), "added line\n").unwrap();
    std::fs::write(repo.join("a b.txt"), "other\n").unwrap();
    run_git(&["add", "-A"], &repo);
    run_git(&["commit", "-q", "-m", "add files"], &repo);
    let hash = run_git(&["rev-parse", "HEAD"], &repo);
    (tmp, repo, hash)
}

#[test]
fn commit_files_returns_unquoted_non_ascii_and_spaced_paths() {
    let (_tmp, repo, hash) = repo_with_non_ascii_commit();

    let mut files = commit_files(&repo, &hash).unwrap();
    files.sort();

    let mut expected = vec![
        repo.join("café.txt").to_string_lossy().into_owned(),
        repo.join("a b.txt").to_string_lossy().into_owned(),
    ];
    expected.sort();
    assert_eq!(files, expected);
}

#[test]
fn commit_file_diff_resolves_a_non_ascii_path() {
    let (_tmp, repo, hash) = repo_with_non_ascii_commit();
    let path = repo.join("café.txt");

    let diff = commit_file_diff(&repo, &hash, path.to_str().unwrap()).unwrap();

    assert!(diff.contains("+added line"), "diff was: {diff}");
}
