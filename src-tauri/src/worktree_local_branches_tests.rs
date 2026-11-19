use super::local_branches::{
    BranchCandidate, BranchUsage, WorktreeEntry, add_worktree_for_branch, branch_usage,
    clear_missing_worktrees, commit_file_diff, commit_files, is_valid_commit_id,
    nergal_branch_candidates, nergal_slug, pending_commit_count, pending_commits, resolve_commit,
    worktree_entries,
};
use super::*;

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
        Fixture { tmp, repo, record }
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().canonicalize().unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        run_git(args, &self.repo)
    }

    fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev])
    }

    fn branch_sha(&self, branch: &str) -> String {
        self.sha(&format!("refs/heads/{branch}"))
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

    /// Local-only branch off main whose commits are dated `base_unix + i`.
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

    fn commit_file(&self, rel: &str, content: &str, subject: &str, unix: i64) -> String {
        let path = self.repo.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        self.git(&["add", rel]);
        run_git_dated(&["commit", "-q", "-m", subject], &self.repo, unix);
        self.git(&["rev-parse", "HEAD"])
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

    fn lock_worktree(&self, path: &Path) {
        self.git(&["worktree", "lock", path.to_str().unwrap()]);
    }

    fn drop_branch_unconditionally(&self, branch: &str, expected: &str) -> BranchOutcome {
        drop_branch(
            &self.repo,
            branch,
            Some(expected),
            DropPolicy::Unconditional,
            &self.record,
        )
    }
}

fn names(candidates: &[BranchCandidate]) -> Vec<&str> {
    candidates.iter().map(|c| c.name.as_str()).collect()
}

fn entry_at<'a>(entries: &'a [WorktreeEntry], path: &Path) -> &'a WorktreeEntry {
    entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("no worktree entry for {}", path.display()))
}

#[test]
fn candidates_come_back_newest_first_with_their_commit_data() {
    let fx = Fixture::new();
    let old = fx.local_branch("nergal/old", &["old work"], 1000);
    let new = fx.local_branch("nergal/new", &["new work"], 2000);

    let got = nergal_branch_candidates(&fx.repo).unwrap();

    assert_eq!(
        got,
        vec![
            BranchCandidate {
                name: "nergal/new".into(),
                sha: new,
                last_commit_unix: 2000,
                last_subject: "new work".into(),
            },
            BranchCandidate {
                name: "nergal/old".into(),
                sha: old,
                last_commit_unix: 1000,
                last_subject: "old work".into(),
            },
        ]
    );
}

#[test]
fn candidates_skip_non_nergal_branches_and_symrefs() {
    let fx = Fixture::new();
    fx.local_branch("nergal/real", &["work"], 1000);
    fx.local_branch("feature/x", &["other"], 1100);
    fx.git(&["symbolic-ref", "refs/heads/nergal/s", "refs/heads/main"]);

    let got = nergal_branch_candidates(&fx.repo).unwrap();

    assert_eq!(names(&got), vec!["nergal/real"]);
}

#[test]
fn a_branch_is_named_exactly_even_when_a_tag_shares_its_name() {
    let fx = Fixture::new();
    fx.local_branch("nergal/t", &["work"], 1000);
    fx.git(&["tag", "nergal/t", "main"]);

    let got = nergal_branch_candidates(&fx.repo).unwrap();

    assert_eq!(names(&got), vec!["nergal/t"]);
}

#[test]
fn a_subject_containing_tabs_is_kept_whole() {
    let fx = Fixture::new();
    fx.local_branch("nergal/tabs", &["a\tb\tc\td"], 1000);

    let got = nergal_branch_candidates(&fx.repo).unwrap();

    assert_eq!(got[0].last_subject, "a\tb\tc\td");
}

#[test]
fn candidates_fail_when_the_path_is_not_a_repo() {
    let tmp = tempfile::tempdir().unwrap();

    assert!(nergal_branch_candidates(tmp.path()).is_err());
}

#[test]
fn a_branch_with_two_local_only_commits_has_two_pending() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one", "two"], 1000);

    assert_eq!(pending_commit_count(&fx.repo, &sha, None), Some(2));
}

#[test]
fn a_pushed_branch_has_zero_pending() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);
    fx.git(&["push", "-q", "-u", "origin", "nergal/a"]);

    assert_eq!(pending_commit_count(&fx.repo, &sha, None), Some(0));
}

#[test]
fn pending_is_measured_against_the_given_target_not_head() {
    let fx = Fixture::new();
    fx.git(&["branch", "develop", "main"]);
    let sha = fx.local_branch("nergal/a", &["one"], 1000);
    fx.git(&["checkout", "-q", "develop"]);
    fx.git(&["merge", "-q", "--ff-only", "nergal/a"]);
    fx.git(&["checkout", "-q", "main"]);

    assert_eq!(
        pending_commit_count(&fx.repo, &sha, Some("develop")),
        Some(0)
    );
    assert_eq!(pending_commit_count(&fx.repo, &sha, None), Some(1));
}

#[test]
fn an_unknown_target_gives_no_pending_count() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);

    assert_eq!(pending_commit_count(&fx.repo, &sha, Some("gone")), None);
}

#[test]
fn pending_commits_list_only_what_the_target_lacks_newest_first() {
    let fx = Fixture::new();
    fx.git(&["checkout", "-q", "-b", "nergal/q", "main"]);
    run_git_dated(
        &["commit", "-q", "--allow-empty", "-m", "pushed"],
        &fx.repo,
        1000,
    );
    fx.git(&["push", "-q", "-u", "origin", "nergal/q"]);
    run_git_dated(
        &["commit", "-q", "--allow-empty", "-m", "second"],
        &fx.repo,
        1001,
    );
    run_git_dated(
        &["commit", "-q", "--allow-empty", "-m", "third"],
        &fx.repo,
        1002,
    );
    let sha = fx.branch_sha("nergal/q");
    fx.git(&["checkout", "-q", "main"]);

    let got = pending_commits(&fx.repo, &sha, None, 10).unwrap();

    let messages: Vec<&str> = got.iter().map(|c| c.message.as_str()).collect();
    assert_eq!(messages, vec!["third", "second"]);
    assert!(sha.starts_with(&got[0].hash));
}

#[test]
fn pending_commits_are_capped_by_the_limit() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one", "two", "three"], 1000);

    let got = pending_commits(&fx.repo, &sha, None, 2).unwrap();

    let messages: Vec<&str> = got.iter().map(|c| c.message.as_str()).collect();
    assert_eq!(messages, vec!["three", "two"]);
}

#[test]
fn pending_commits_fail_for_an_unknown_target() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/a", &["one"], 1000);

    assert!(pending_commits(&fx.repo, &sha, Some("gone"), 10).is_err());
}

#[test]
fn commit_ids_are_valid_only_as_4_to_64_hex_digits() {
    let forty = "a".repeat(40);
    let too_long = "a".repeat(65);

    assert!(is_valid_commit_id("abc1"));
    assert!(is_valid_commit_id(&forty));
    assert!(!is_valid_commit_id("abc"));
    assert!(!is_valid_commit_id(&too_long));
    assert!(!is_valid_commit_id("--output=/tmp/x"));
    assert!(!is_valid_commit_id("abcg"));
}

#[test]
fn resolve_commit_expands_a_short_prefix_to_the_full_id() {
    let fx = Fixture::new();
    let full = fx.sha("HEAD");

    assert_eq!(resolve_commit(&fx.repo, &full[..8]).unwrap(), full);
}

#[test]
fn resolve_commit_accepts_an_uppercase_prefix() {
    let fx = Fixture::new();
    let full = fx.sha("HEAD");

    assert_eq!(
        resolve_commit(&fx.repo, &full[..8].to_uppercase()).unwrap(),
        full
    );
}

#[test]
fn resolve_commit_refuses_a_hex_looking_branch_name() {
    let fx = Fixture::new();
    fx.git(&["branch", "beef", "main"]);

    assert!(resolve_commit(&fx.repo, "beef").is_err());
}

#[test]
fn resolve_commit_refuses_an_option_like_id_without_running_git() {
    let tmp = tempfile::tempdir().unwrap();

    assert!(resolve_commit(tmp.path(), "--output=x").is_err());
}

#[test]
fn nergal_slug_accepts_only_a_single_safe_segment_under_nergal() {
    assert_eq!(nergal_slug("nergal/a-1"), Some("a-1"));
    assert_eq!(nergal_slug("nergal/a.b_c"), Some("a.b_c"));
    assert_eq!(nergal_slug("nergal/../x"), None);
    assert_eq!(nergal_slug("nergal/a/b"), None);
    assert_eq!(nergal_slug("nergal/.x"), None);
    assert_eq!(nergal_slug("nergal/"), None);
    assert_eq!(nergal_slug("feature/x"), None);
}

#[test]
fn commit_files_lists_only_that_commits_files_joined_to_the_base() {
    let fx = Fixture::new();
    fx.commit_file("earlier.txt", "e\n", "earlier", 1000);
    fx.commit_file("a.txt", "a\n", "touch a", 1001);
    let hash = fx.commit_file("dir/b.txt", "b\n", "touch b", 1002);

    let got = commit_files(&fx.repo, &hash[..8]).unwrap();

    assert_eq!(got, vec![fx.repo.join("dir/b.txt").display().to_string()]);
}

#[test]
fn commit_file_diff_shows_the_commit_not_a_later_uncommitted_change() {
    let fx = Fixture::new();
    fx.commit_file("f.txt", "base\n", "first", 1000);
    let hash = fx.commit_file("f.txt", "base\ncommitted\n", "second", 1001);
    std::fs::write(fx.repo.join("f.txt"), "base\ncommitted\nuncommitted\n").unwrap();

    let diff = commit_file_diff(
        &fx.repo,
        &hash,
        &fx.repo.join("f.txt").display().to_string(),
    )
    .unwrap();

    assert!(diff.contains("+committed"), "diff was: {diff}");
    assert!(!diff.contains("uncommitted"), "diff was: {diff}");
}

#[test]
fn commit_file_diff_of_a_root_commit_shows_the_full_content() {
    let fx = Fixture::new();
    fx.git(&["checkout", "-q", "--orphan", "rooted"]);
    let hash = fx.commit_file("r.txt", "rootline\n", "root", 1000);
    fx.git(&["checkout", "-q", "-f", "main"]);

    let diff = commit_file_diff(&fx.repo, &hash, "r.txt").unwrap();

    assert!(diff.contains("+rootline"), "diff was: {diff}");
}

#[test]
fn commit_file_diff_of_a_merge_is_the_first_parent_diff() {
    let fx = Fixture::new();
    fx.git(&["checkout", "-q", "-b", "side", "main"]);
    fx.commit_file("s.txt", "side\n", "side work", 1000);
    fx.git(&["checkout", "-q", "main"]);
    fx.commit_file("m.txt", "mainonly\n", "main work", 1001);
    run_git_dated(
        &["merge", "-q", "--no-ff", "-m", "merge", "side"],
        &fx.repo,
        1002,
    );
    let merge = fx.sha("HEAD");

    let side = commit_file_diff(&fx.repo, &merge, "s.txt").unwrap();
    let main_only = commit_file_diff(&fx.repo, &merge, "m.txt").unwrap();

    assert!(side.contains("+side"), "diff was: {side}");
    assert_eq!(main_only, "");
}

#[test]
fn commit_file_diff_treats_the_path_literally() {
    let fx = Fixture::new();
    let hash = fx.commit_file("x.ts", "export {}\n", "add ts", 1000);

    let diff = commit_file_diff(&fx.repo, &hash, ":(glob)*.ts").unwrap();

    assert_eq!(diff, "");
}

#[test]
fn commit_file_diff_refuses_an_option_like_id_and_creates_nothing() {
    let fx = Fixture::new();
    fx.commit_file("f.txt", "a\n", "add", 1000);
    let target = fx.root().join("x");
    let id = format!("--output={}", target.display());

    let result = commit_file_diff(&fx.repo, &id, "f.txt");

    assert!(result.is_err());
    assert!(!target.exists());
}

#[test]
fn commit_file_diff_refuses_paths_that_escape_the_base() {
    let fx = Fixture::new();
    let hash = fx.commit_file("f.txt", "a\n", "add", 1000);

    assert!(commit_file_diff(&fx.repo, &hash, "../outside.txt").is_err());
    assert!(commit_file_diff(&fx.repo, &hash, "/etc/hostname").is_err());
}

#[test]
fn worktree_entries_report_branch_lock_and_prunable_state() {
    let fx = Fixture::new();
    let live = fx.add_worktree("nergal/live", "live");
    let locked = fx.add_worktree("nergal/locked", "locked");
    fx.lock_worktree(&locked);
    let gone = fx.add_worktree("nergal/gone", "gone");
    std::fs::remove_dir_all(&gone).unwrap();
    let detached = fx.worktree_path("detached");
    fx.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        detached.to_str().unwrap(),
    ]);

    let entries = worktree_entries(&fx.repo).unwrap();

    assert_eq!(
        entry_at(&entries, &fx.repo),
        &WorktreeEntry {
            path: fx.repo.clone(),
            branch: Some("main".into()),
            locked: false,
            prunable: false,
        }
    );
    assert_eq!(
        entry_at(&entries, &live),
        &WorktreeEntry {
            path: live.clone(),
            branch: Some("nergal/live".into()),
            locked: false,
            prunable: false,
        }
    );
    assert!(entry_at(&entries, &locked).locked);
    assert!(!entry_at(&entries, &locked).prunable);
    assert!(entry_at(&entries, &gone).prunable);
    assert_eq!(
        entry_at(&entries, &gone).branch.as_deref(),
        Some("nergal/gone")
    );
    assert_eq!(entry_at(&entries, &detached).branch, None);
}

#[test]
fn a_branch_checked_out_in_an_existing_worktree_is_in_use() {
    let fx = Fixture::new();
    fx.add_worktree("nergal/x", "x");

    assert_eq!(branch_usage(&fx.repo, "nergal/x"), BranchUsage::InUse);
}

#[test]
fn a_locked_worktree_with_a_missing_directory_still_counts_as_in_use() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/x", "x");
    fx.lock_worktree(&wt);
    std::fs::remove_dir_all(&wt).unwrap();

    assert_eq!(branch_usage(&fx.repo, "nergal/x"), BranchUsage::InUse);
}

#[test]
fn a_branch_being_rebased_is_in_use() {
    let fx = Fixture::new();
    fx.local_branch("nergal/x", &["work"], 1000);
    let rebase = fx.repo.join(".git").join("rebase-merge");
    std::fs::create_dir_all(&rebase).unwrap();
    std::fs::write(rebase.join("head-name"), "refs/heads/nergal/x\n").unwrap();

    assert_eq!(branch_usage(&fx.repo, "nergal/x"), BranchUsage::InUse);
}

#[test]
fn a_branch_whose_worktree_directory_was_deleted_is_reported_missing() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/x", "x");
    std::fs::remove_dir_all(&wt).unwrap();

    assert_eq!(
        branch_usage(&fx.repo, "nergal/x"),
        BranchUsage::MissingWorktree { paths: vec![wt] }
    );
}

#[test]
fn a_branch_no_worktree_holds_is_free() {
    let fx = Fixture::new();
    fx.local_branch("nergal/x", &["work"], 1000);

    assert_eq!(branch_usage(&fx.repo, "nergal/x"), BranchUsage::Free);
}

#[test]
fn clearing_removes_only_the_missing_unlocked_entry_of_that_branch() {
    let fx = Fixture::new();
    let target = fx.add_worktree("nergal/y", "y");
    let other = fx.add_worktree("nergal/z", "z");
    let locked = fx.add_worktree("nergal/l", "l");
    fx.lock_worktree(&locked);
    std::fs::remove_dir_all(&target).unwrap();
    std::fs::remove_dir_all(&other).unwrap();
    std::fs::remove_dir_all(&locked).unwrap();

    let removed = clear_missing_worktrees(&fx.repo, "nergal/y").unwrap();

    let entries = worktree_entries(&fx.repo).unwrap();
    assert_eq!(removed, vec![target.clone()]);
    assert!(entries.iter().all(|e| e.path != target));
    assert!(entries.iter().any(|e| e.path == other));
    assert!(entries.iter().any(|e| e.path == locked));
}

#[test]
fn clearing_leaves_a_locked_entry_of_the_same_branch_alone() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("nergal/l", "l");
    fx.lock_worktree(&wt);
    std::fs::remove_dir_all(&wt).unwrap();

    let removed = clear_missing_worktrees(&fx.repo, "nergal/l").unwrap();

    assert!(removed.is_empty());
    assert!(
        worktree_entries(&fx.repo)
            .unwrap()
            .iter()
            .any(|e| e.path == wt)
    );
}

#[test]
fn the_added_worktree_is_attached_to_the_branch_without_moving_it() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/s", &["work"], 1000);

    let path = add_worktree_for_branch(&fx.repo, "s").unwrap();

    assert_eq!(path, fx.repo.join(".worktrees/nergal/s"));
    assert_eq!(
        run_git(&["symbolic-ref", "HEAD"], &path),
        "refs/heads/nergal/s"
    );
    assert_eq!(run_git(&["rev-parse", "HEAD"], &path), sha);
    assert_eq!(fx.branch_sha("nergal/s"), sha);
}

#[test]
fn the_added_worktree_attaches_to_the_branch_even_when_a_tag_shares_its_name() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/s", &["work"], 1000);
    fx.git(&["tag", "nergal/s", "main"]);

    let path = add_worktree_for_branch(&fx.repo, "s").unwrap();

    assert_eq!(
        run_git(&["symbolic-ref", "HEAD"], &path),
        "refs/heads/nergal/s"
    );
    assert_eq!(run_git(&["rev-parse", "HEAD"], &path), sha);
    assert_eq!(fx.branch_sha("nergal/s"), sha);
}

#[test]
fn adding_a_worktree_over_an_existing_directory_is_refused_and_names_the_path() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["work"], 1000);
    let dir = fx.repo.join(".worktrees/nergal/s");
    std::fs::create_dir_all(&dir).unwrap();

    let err = add_worktree_for_branch(&fx.repo, "s").unwrap_err();

    assert!(
        err.to_string().contains(&dir.display().to_string()),
        "error was: {err}"
    );
}

#[cfg(unix)]
#[test]
fn adding_a_worktree_over_a_dangling_symlink_is_refused() {
    let fx = Fixture::new();
    fx.local_branch("nergal/s", &["work"], 1000);
    let link = fx.repo.join(".worktrees/nergal/s");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(fx.root().join("nowhere"), &link).unwrap();

    assert!(add_worktree_for_branch(&fx.repo, "s").is_err());
}

#[test]
fn adding_a_worktree_for_a_branch_checked_out_elsewhere_fails() {
    let fx = Fixture::new();
    fx.add_worktree("nergal/s", "elsewhere");

    assert!(add_worktree_for_branch(&fx.repo, "s").is_err());
}

#[test]
fn an_unconditional_drop_with_the_right_sha_deletes_a_local_only_branch_and_records_it() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/d", &["local only"], 1000);

    let outcome = fx.drop_branch_unconditionally("nergal/d", &sha);

    assert_eq!(outcome, BranchOutcome::Deleted { sha: sha.clone() });
    assert!(!fx.exists("nergal/d"));
    let record = std::fs::read_to_string(&fx.record).unwrap();
    assert_eq!(record.lines().count(), 1);
    assert!(
        record.trim_end().ends_with(&format!("\tnergal/d\t{sha}")),
        "record was: {record}"
    );
}

#[test]
fn a_stale_expected_sha_keeps_the_branch_and_leaves_no_record() {
    let fx = Fixture::new();
    fx.local_branch("nergal/d", &["local only"], 1000);
    let stale = fx.sha("main");

    let outcome = fx.drop_branch_unconditionally("nergal/d", &stale);

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Moved
        }
    );
    assert!(fx.exists("nergal/d"));
    assert!(!fx.record.exists());
}

#[test]
fn the_require_safe_policy_still_keeps_a_local_only_branch_when_the_sha_matches() {
    let fx = Fixture::new();
    let sha = fx.local_branch("nergal/d", &["local only"], 1000);

    let outcome = drop_branch(
        &fx.repo,
        "nergal/d",
        Some(&sha),
        DropPolicy::RequireSafe { target: None },
        &fx.record,
    );

    assert_eq!(
        outcome,
        BranchOutcome::Kept {
            reason: KeptReason::Unshared
        }
    );
    assert!(fx.exists("nergal/d"));
}
