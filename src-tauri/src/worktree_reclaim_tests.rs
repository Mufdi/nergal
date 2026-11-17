use super::*;
use std::ffi::OsString;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::sync::Mutex;

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn gid() -> u32 {
    unsafe { libc::getgid() }
}

fn run_git(args: &[&str], cwd: &Path) {
    let out = git().args(args).current_dir(cwd).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn mode_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path).unwrap().mode() & 0o777
}

/// Makes every directory of the tree `u+rwx`, fixing each mode before reading it.
fn open_all_dirs(dir: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            open_all_dirs(&entry.path());
        }
    }
}

/// Lets `tempdir` clean up even when an assertion fails while dirs are sealed.
struct Unseal(PathBuf);

impl Drop for Unseal {
    fn drop(&mut self) {
        open_all_dirs(&self.0);
    }
}

struct Fixture {
    _unseal: Unseal,
    _tmp: tempfile::TempDir,
    repo: PathBuf,
    wt: PathBuf,
    sealed: PathBuf,
}

fn init_repo(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    run_git(&["init", "-q", "-b", "main"], repo);
    run_git(
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ],
        repo,
    );
}

fn add_worktree(repo: &Path, parent: &str, slug: &str) -> PathBuf {
    let wt = repo.join(".worktrees").join(parent).join(slug);
    run_git(
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("{parent}/{slug}"),
            wt.to_str().unwrap(),
        ],
        repo,
    );
    wt
}

fn seal_cache(wt: &Path) -> PathBuf {
    let sealed = wt.join("cache");
    std::fs::create_dir(&sealed).unwrap();
    std::fs::write(sealed.join("artifact"), "x").unwrap();
    set_mode(&sealed, 0o555);
    sealed
}

impl Fixture {
    fn new() -> Self {
        Self::under("nergal")
    }

    fn under(parent: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let repo = root.join("repo");
        init_repo(&repo);
        let wt = add_worktree(&repo, parent, "x");
        let sealed = seal_cache(&wt);
        Fixture {
            _unseal: Unseal(root),
            _tmp: tmp,
            repo,
            wt,
            sealed,
        }
    }

    fn is_registered(&self) -> bool {
        registered_worktrees(&self.repo).contains(&self.wt)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Own(PathBuf),
    Status,
    Docker(PathBuf, u64, u64, u32, u32),
}

#[derive(Clone, Copy)]
enum Unblock {
    MakesWritable,
    NoEffect,
    Fails,
}

struct Fake {
    calls: Mutex<Vec<Call>>,
    status: Result<(), ReclaimIssue>,
    unblock: Unblock,
}

impl Fake {
    fn new(status: Result<(), ReclaimIssue>, unblock: Unblock) -> Self {
        Fake {
            calls: Mutex::new(Vec::new()),
            status,
            unblock,
        }
    }

    fn calls(&self) -> std::sync::MutexGuard<'_, Vec<Call>> {
        self.calls.lock().unwrap()
    }
}

impl Reclaimer for Fake {
    fn unblock_own(&self, dir: &Path, _uid: u32) {
        self.calls().push(Call::Own(dir.to_path_buf()));
    }

    fn docker_status(&self) -> Result<(), ReclaimIssue> {
        self.calls().push(Call::Status);
        self.status
    }

    fn docker_unblock(&self, target: &ReclaimTarget, uid: u32, gid: u32) -> bool {
        self.calls().push(Call::Docker(
            target.path.clone(),
            target.dev,
            target.ino,
            uid,
            gid,
        ));
        match self.unblock {
            Unblock::MakesWritable => {
                open_all_dirs(&target.path);
                true
            }
            Unblock::NoEffect => true,
            Unblock::Fails => false,
        }
    }
}

/// Keeps the real default `unblock_own` so the unprivileged step runs for real.
struct DefaultOwn {
    docker_calls: Mutex<u32>,
}

impl Reclaimer for DefaultOwn {
    fn docker_status(&self) -> Result<(), ReclaimIssue> {
        *self.docker_calls.lock().unwrap() += 1;
        Ok(())
    }

    fn docker_unblock(&self, _target: &ReclaimTarget, _uid: u32, _gid: u32) -> bool {
        *self.docker_calls.lock().unwrap() += 1;
        true
    }
}

fn docker_calls(fake: &Fake) -> Vec<(PathBuf, u64, u64, u32, u32)> {
    fake.calls()
        .iter()
        .filter_map(|c| match c {
            Call::Docker(p, d, i, u, g) => Some((p.clone(), *d, *i, *u, *g)),
            _ => None,
        })
        .collect()
}

fn assert_manual_error(err: &str, fx: &Fixture, reason: &str) {
    assert!(err.contains(&fx.wt.display().to_string()), "{err}");
    assert!(err.contains("sudo rm -rf"), "{err}");
    assert!(err.contains(reason), "{err}");
}

// ---- group 1: guard and command line ----

#[test]
fn reclaim_target_accepts_a_registered_user_owned_worktree() {
    let fx = Fixture::new();
    let got = reclaim_target(&fx.repo, &fx.wt, uid(), std::slice::from_ref(&fx.wt)).unwrap();
    assert_eq!(got.path, fx.wt);
}

#[test]
fn reclaim_target_carries_the_dev_and_ino_of_the_canonical_path() {
    let fx = Fixture::new();
    let via_dots = fx.repo.join(".worktrees/nergal/../nergal/x");

    let got = reclaim_target(&fx.repo, &via_dots, uid(), std::slice::from_ref(&fx.wt)).unwrap();

    let meta = std::fs::symlink_metadata(&fx.wt).unwrap();
    assert_eq!(got.path, fx.wt);
    assert_eq!((got.dev, got.ino), (meta.dev(), meta.ino()));
}

#[test]
fn reclaim_target_rejects_wrong_depth_or_location() {
    let fx = Fixture::new();
    std::fs::create_dir(fx.wt.join("sub")).unwrap();
    std::fs::create_dir(fx.repo.join("src")).unwrap();
    let missing = fx.repo.join(".worktrees/nergal/missing");
    let cases = [
        fx.repo.clone(),
        fx.repo.join(".worktrees"),
        fx.repo.join(".worktrees/nergal"),
        fx.wt.join("sub"),
        fx.repo.join("src"),
        PathBuf::from("/"),
        missing,
    ];
    let registered: Vec<PathBuf> = cases.iter().filter_map(|p| p.canonicalize().ok()).collect();
    for case in &cases {
        assert_eq!(
            reclaim_target(&fx.repo, case, uid(), &registered).err(),
            Some(ReclaimIssue::GuardRejected),
            "{}",
            case.display()
        );
    }
}

#[test]
fn reclaim_target_rejects_a_symlinked_worktrees_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let repo = root.join("repo");
    let elsewhere = root.join("elsewhere");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(elsewhere.join("nergal/x")).unwrap();
    symlink(&elsewhere, repo.join(".worktrees")).unwrap();
    let registered = vec![elsewhere.join("nergal/x")];

    let got = reclaim_target(&repo, &repo.join(".worktrees/nergal/x"), uid(), &registered);

    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}

#[test]
fn reclaim_target_rejects_a_symlinked_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let repo = root.join("repo");
    let target = root.join("elsewhere");
    std::fs::create_dir_all(repo.join(".worktrees/nergal")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let link = repo.join(".worktrees/nergal/link");
    symlink(&target, &link).unwrap();
    let registered = vec![target.clone(), link.clone()];

    let got = reclaim_target(&repo, &link, uid(), &registered);

    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}

#[test]
fn reclaim_target_rejects_a_directory_absent_from_the_snapshot() {
    let fx = Fixture::new();
    let stray = fx.repo.join(".worktrees/nergal/stray");
    std::fs::create_dir(&stray).unwrap();

    let got = reclaim_target(&fx.repo, &stray, uid(), std::slice::from_ref(&fx.wt));

    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}

#[test]
fn reclaim_target_rejects_traversal_and_mount_syntax_characters() {
    let fx = Fixture::new();
    std::fs::create_dir(fx.repo.join("src")).unwrap();
    let nergal = fx.repo.join(".worktrees/nergal");
    for slug in ["a,b", "a:b", "a\"b"] {
        std::fs::create_dir(nergal.join(slug)).unwrap();
    }
    let cases = [
        nergal.join("../../src"),
        nergal.join("a,b"),
        nergal.join("a:b"),
        nergal.join("a\"b"),
    ];
    let registered: Vec<PathBuf> = cases.iter().map(|p| p.canonicalize().unwrap()).collect();
    for case in &cases {
        assert_eq!(
            reclaim_target(&fx.repo, case, uid(), &registered).err(),
            Some(ReclaimIssue::GuardRejected),
            "{}",
            case.display()
        );
    }
}

#[test]
fn reclaim_target_rejects_a_worktree_owned_by_another_uid() {
    let fx = Fixture::new();
    let got = reclaim_target(&fx.repo, &fx.wt, uid() + 1, std::slice::from_ref(&fx.wt));
    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}

#[test]
fn registered_worktrees_lists_a_real_git_worktree_add() {
    let fx = Fixture::new();
    let listed = registered_worktrees(&fx.repo);
    assert!(listed.contains(&fx.wt), "{listed:?}");
    assert!(listed.contains(&fx.repo), "{listed:?}");
}

#[test]
fn docker_unblock_args_is_the_pinned_script_vector() {
    let target = ReclaimTarget {
        path: PathBuf::from("/data/wt/x"),
        dev: 66306,
        ino: 123456,
    };
    let got = docker_unblock_args(
        "nergal-reclaim-7-3",
        "unix:///var/run/docker.sock",
        &target,
        1000,
        1001,
    );
    let want: Vec<OsString> = [
        "-H",
        "unix:///var/run/docker.sock",
        "run",
        "--rm",
        "--name",
        "nergal-reclaim-7-3",
        "--pull",
        "never",
        "--network",
        "none",
        "--user",
        "0:0",
        "--entrypoint",
        "sh",
        "--mount",
        "type=bind,source=/data/wt/x,target=/w",
        "alpine:3",
        "-c",
        RECLAIM_SCRIPT,
        "reclaim",
        "66306:123456",
        "1000:1001",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    assert_eq!(got, want);
}

#[test]
fn docker_supported_only_for_rootful_local_docker() {
    let ok_version = "Docker version 24.0.7, build afdd53b";
    let ok_options = r#"["name=apparmor","name=seccomp,profile=builtin"]"#;
    let ok_endpoint = "unix:///var/run/docker.sock";
    let cases = [
        (
            "rootful unix docker",
            ok_version,
            ok_options,
            ok_endpoint,
            true,
        ),
        (
            "podman",
            "podman version 4.9.3",
            ok_options,
            ok_endpoint,
            false,
        ),
        (
            "rootless",
            ok_version,
            r#"["name=seccomp,profile=builtin","name=rootless"]"#,
            ok_endpoint,
            false,
        ),
        (
            "userns remap",
            ok_version,
            r#"["name=seccomp,profile=builtin","name=userns"]"#,
            ok_endpoint,
            false,
        ),
        (
            "tcp endpoint",
            ok_version,
            ok_options,
            "tcp://10.0.0.1:2375",
            false,
        ),
    ];
    for (id, version, options, endpoint, want) in cases {
        assert_eq!(docker_supported(version, options, endpoint), want, "{id}");
    }
}

#[test]
fn blocked_dirs_is_false_on_a_fresh_tree() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("a/b")).unwrap();
    assert!(!blocked_dirs(tmp.path()));
}

#[test]
fn blocked_dirs_is_true_for_unwritable_or_unreadable_subdirs() {
    for (id, mode) in [("read-only", 0o555), ("unreadable", 0o000)] {
        let tmp = tempfile::tempdir().unwrap();
        let _unseal = Unseal(tmp.path().to_path_buf());
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        set_mode(&sub, mode);
        assert!(blocked_dirs(tmp.path()), "{id}");
    }
}

#[test]
fn unblock_own_dirs_makes_user_owned_sealed_dirs_deletable() {
    let tmp = tempfile::tempdir().unwrap();
    let _unseal = Unseal(tmp.path().to_path_buf());
    let tree = tmp.path().join("tree");
    let read_only = tree.join("ro");
    let closed = tree.join("closed");
    std::fs::create_dir_all(&read_only).unwrap();
    std::fs::write(read_only.join("f"), "x").unwrap();
    std::fs::create_dir_all(closed.join("child")).unwrap();
    std::fs::write(closed.join("child/f"), "x").unwrap();
    set_mode(&read_only, 0o555);
    set_mode(&closed, 0o000);

    unblock_own_dirs(&tree, uid());

    std::fs::remove_dir_all(&tree).unwrap();
    assert!(!tree.exists());
}

#[test]
fn unblock_own_dirs_never_follows_a_symlinked_root() {
    let tmp = tempfile::tempdir().unwrap();
    let _unseal = Unseal(tmp.path().to_path_buf());
    let target = tmp.path().join("elsewhere");
    let inner = target.join("ro");
    std::fs::create_dir_all(&inner).unwrap();
    set_mode(&inner, 0o555);
    let link = tmp.path().join("link");
    symlink(&target, &link).unwrap();

    unblock_own_dirs(&link, uid());

    assert_eq!(mode_of(&inner), 0o555);
}

#[test]
fn unblock_own_dirs_skips_directories_of_another_uid() {
    let tmp = tempfile::tempdir().unwrap();
    let _unseal = Unseal(tmp.path().to_path_buf());
    let tree = tmp.path().join("tree");
    let ro = tree.join("ro");
    std::fs::create_dir_all(&ro).unwrap();
    set_mode(&ro, 0o555);

    unblock_own_dirs(&tree, uid() + 1);

    assert_eq!(mode_of(&ro), 0o555);
}

#[test]
fn reclaim_issue_reasons_are_the_user_facing_strings() {
    let cases = [
        (ReclaimIssue::GuardRejected, "guard rejected"),
        (ReclaimIssue::InProgress, "removal in progress"),
        (ReclaimIssue::DockerNotFound, "docker not found"),
        (ReclaimIssue::DockerUnsupported, "docker unsupported"),
        (ReclaimIssue::DockerFailed, "docker failed"),
        (ReclaimIssue::Ineffective, "reclaim ineffective"),
    ];
    for (issue, want) in cases {
        assert_eq!(issue.reason(), want);
    }
}

#[test]
fn claim_removal_on_a_missing_path_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let got = claim_removal(&tmp.path().join("missing"));
    assert_eq!(got.err(), Some(ClaimError::Gone));
}

#[test]
fn claim_removal_allows_one_holder_until_it_is_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();

    let first = claim_removal(&path).unwrap();
    assert_eq!(claim_removal(&path).err(), Some(ClaimError::InProgress));

    drop(first);
    assert!(claim_removal(&path).is_ok());
}

// ---- group 2: reclaim and retry ----

#[test]
fn user_owned_read_only_dirs_are_removed_without_docker() {
    let fx = Fixture::new();
    let r = DefaultOwn {
        docker_calls: Mutex::new(0),
    };

    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap();

    assert!(!fx.wt.exists());
    assert!(!fx.is_registered());
    assert_eq!(*r.docker_calls.lock().unwrap(), 0);
}

#[test]
fn docker_step_that_makes_dirs_writable_lets_the_retry_succeed() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::MakesWritable);
    let meta = std::fs::symlink_metadata(&fx.wt).unwrap();

    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap();

    assert_eq!(
        docker_calls(&r),
        vec![(fx.wt.clone(), meta.dev(), meta.ino(), uid(), gid())]
    );
    assert!(!fx.wt.exists());
    assert!(!fx.is_registered());
}

#[test]
fn docker_exiting_zero_without_effect_reports_reclaim_ineffective() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::NoEffect);

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r))
        .unwrap_err()
        .to_string();

    assert_manual_error(&err, &fx, "reclaim ineffective");
    assert!(
        err.contains("stop any container that mounts this worktree"),
        "{err}"
    );
    assert!(fx.sealed.exists());
}

#[test]
fn a_failed_docker_run_reports_docker_failed() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::Fails);

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r))
        .unwrap_err()
        .to_string();

    assert_manual_error(&err, &fx, "docker failed");
    assert_eq!(docker_calls(&r).len(), 1);
}

#[test]
fn an_unavailable_docker_never_runs_a_container_and_names_the_reason() {
    for (status, reason) in [
        (ReclaimIssue::DockerNotFound, "docker not found"),
        (ReclaimIssue::DockerUnsupported, "docker unsupported"),
    ] {
        let fx = Fixture::new();
        let r = Fake::new(Err(status), Unblock::MakesWritable);

        let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r))
            .unwrap_err()
            .to_string();

        assert_manual_error(&err, &fx, reason);
        assert_eq!(docker_calls(&r), vec![], "{reason}");
    }
}

#[test]
fn a_worktree_outside_the_nergal_dir_is_rejected_before_docker() {
    let fx = Fixture::under("other");
    let r = Fake::new(Ok(()), Unblock::MakesWritable);

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r))
        .unwrap_err()
        .to_string();

    assert_manual_error(&err, &fx, "guard rejected");
    assert_eq!(docker_calls(&r), vec![]);
}

#[test]
fn a_concurrent_removal_of_the_same_path_fails_without_touching_anything() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::MakesWritable);
    let _held = claim_removal(&fx.wt).unwrap();

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r))
        .unwrap_err()
        .to_string();

    assert!(err.contains("removal in progress"), "{err}");
    assert_eq!(*r.calls(), vec![]);
    assert!(fx.sealed.exists());
    assert!(fx.is_registered());
}

#[test]
fn a_worktree_dir_that_is_already_gone_is_pruned_and_ok() {
    let fx = Fixture::new();
    open_all_dirs(&fx.wt);
    std::fs::remove_dir_all(&fx.wt).unwrap();
    let r = Fake::new(Ok(()), Unblock::MakesWritable);

    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap();

    assert!(!fx.is_registered());
    assert_eq!(*r.calls(), vec![]);
}

#[test]
fn without_a_reclaimer_the_permission_error_is_unchanged() {
    let fx = Fixture::new();

    let err = remove_worktree_with(&fx.repo, &fx.wt, None)
        .unwrap_err()
        .to_string();

    assert!(err.contains(&fx.wt.display().to_string()), "{err}");
    assert!(err.contains("sudo rm -rf"), "{err}");
    assert!(!err.contains("reclaim"), "{err}");
}

#[test]
fn the_docker_step_receives_the_worktrees_own_dev_and_ino() {
    let fx = Fixture::new();
    let meta = std::fs::symlink_metadata(&fx.wt).unwrap();
    let r = Fake::new(Ok(()), Unblock::Fails);

    let _ = remove_worktree_with(&fx.repo, &fx.wt, Some(&r));

    let calls = docker_calls(&r);
    assert_eq!(calls.len(), 1);
    assert_eq!((calls[0].1, calls[0].2), (meta.dev(), meta.ino()));
}

#[test]
fn unblock_own_dirs_turns_a_user_owned_0777_dir_into_0755() {
    let tmp = tempfile::tempdir().unwrap();
    let _unseal = Unseal(tmp.path().to_path_buf());
    let tree = tmp.path().join("tree");
    let open = tree.join("open");
    std::fs::create_dir_all(&open).unwrap();
    set_mode(&open, 0o777);

    unblock_own_dirs(&tree, uid());

    assert_eq!(mode_of(&open), 0o755);
}

#[test]
fn unblock_own_dirs_keeps_the_sticky_bit_and_drops_setgid() {
    let tmp = tempfile::tempdir().unwrap();
    let _unseal = Unseal(tmp.path().to_path_buf());
    let tree = tmp.path().join("tree");
    let sticky = tree.join("sticky");
    std::fs::create_dir_all(&sticky).unwrap();
    set_mode(&sticky, 0o3555);

    unblock_own_dirs(&tree, uid());

    let mode = std::fs::symlink_metadata(&sticky).unwrap().mode() & 0o7777;
    assert_eq!(mode, 0o1755);
}

// ---- group 3: failures keep the session ----

fn reclaim_failure_issue(err: &anyhow::Error) -> Option<ReclaimIssue> {
    err.downcast_ref::<ReclaimFailure>().map(|f| f.issue)
}

#[test]
fn every_reclaim_path_error_is_a_reclaim_failure_with_its_issue() {
    let cases = [
        (Ok(()), Unblock::Fails, ReclaimIssue::DockerFailed),
        (Ok(()), Unblock::NoEffect, ReclaimIssue::Ineffective),
        (
            Err(ReclaimIssue::DockerNotFound),
            Unblock::MakesWritable,
            ReclaimIssue::DockerNotFound,
        ),
        (
            Err(ReclaimIssue::DockerUnsupported),
            Unblock::MakesWritable,
            ReclaimIssue::DockerUnsupported,
        ),
    ];
    for (status, unblock, issue) in cases {
        let fx = Fixture::new();
        let r = Fake::new(status, unblock);

        let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

        assert!(is_reclaim_failure(&err), "{}: {err}", issue.reason());
        assert_eq!(reclaim_failure_issue(&err), Some(issue));
        assert!(err.to_string().contains(issue.reason()), "{err}");
    }
}

#[test]
fn a_guard_rejection_is_a_reclaim_failure() {
    let fx = Fixture::under("other");
    let r = Fake::new(Ok(()), Unblock::MakesWritable);

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

    assert!(is_reclaim_failure(&err), "{err}");
    assert_eq!(
        reclaim_failure_issue(&err),
        Some(ReclaimIssue::GuardRejected)
    );
}

#[test]
fn a_concurrent_removal_is_a_reclaim_failure() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::MakesWritable);
    let _held = claim_removal(&fx.wt).unwrap();

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

    assert!(is_reclaim_failure(&err), "{err}");
    assert_eq!(reclaim_failure_issue(&err), Some(ReclaimIssue::InProgress));
}

#[test]
fn a_permission_error_without_a_reclaimer_is_not_a_reclaim_failure() {
    let fx = Fixture::new();

    let err = remove_worktree_with(&fx.repo, &fx.wt, None).unwrap_err();

    assert!(!is_reclaim_failure(&err), "{err}");
}

#[test]
fn an_unrelated_error_is_not_a_reclaim_failure() {
    assert!(!is_reclaim_failure(&anyhow::anyhow!(
        "git worktree prune failed"
    )));
}

#[test]
fn reclaim_ineffective_always_carries_the_manual_remedy() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::NoEffect);

    let err = remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

    assert_eq!(reclaim_failure_issue(&err), Some(ReclaimIssue::Ineffective));
    assert!(err.to_string().contains("sudo rm -rf"), "{err}");
    assert!(
        err.to_string().contains(&fx.wt.display().to_string()),
        "{err}"
    );
}

#[test]
fn a_failed_reclaim_leaves_the_dir_and_drops_the_git_registration() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::Fails);

    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

    assert!(fx.sealed.exists());
    assert!(!fx.is_registered());
}

// ---- group 4: retry with the recorded path ----

#[test]
fn reclaim_snapshot_adds_the_recorded_path_that_git_no_longer_lists() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::Fails);
    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();
    assert!(!registered_worktrees(&fx.repo).contains(&fx.wt));

    let snapshot = reclaim_snapshot(&fx.repo, &fx.wt);

    assert!(snapshot.contains(&fx.wt), "{snapshot:?}");
    assert!(
        reclaim_target(&fx.repo, &fx.wt, uid(), &snapshot).is_ok(),
        "the guard must accept the recorded path on retry"
    );
}

#[test]
fn without_the_recorded_path_the_guard_rejects_an_unregistered_worktree() {
    let fx = Fixture::new();
    let r = Fake::new(Ok(()), Unblock::Fails);
    remove_worktree_with(&fx.repo, &fx.wt, Some(&r)).unwrap_err();

    let got = reclaim_target(&fx.repo, &fx.wt, uid(), &registered_worktrees(&fx.repo));

    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}

#[test]
fn reclaim_snapshot_keeps_git_listed_worktrees() {
    let fx = Fixture::new();

    let snapshot = reclaim_snapshot(&fx.repo, &fx.wt);

    assert!(snapshot.contains(&fx.wt), "{snapshot:?}");
    assert!(snapshot.contains(&fx.repo), "{snapshot:?}");
    assert_eq!(snapshot.iter().filter(|p| **p == fx.wt).count(), 1);
}

#[test]
fn reclaim_snapshot_ignores_a_recorded_path_that_does_not_exist() {
    let fx = Fixture::new();
    let missing = fx.repo.join(".worktrees/nergal/missing");

    let snapshot = reclaim_snapshot(&fx.repo, &missing);

    assert!(
        !snapshot.iter().any(|p| p.ends_with("missing")),
        "{snapshot:?}"
    );
}

#[test]
fn a_recorded_symlink_is_canonicalized_so_it_cannot_widen_the_guard() {
    let fx = Fixture::new();
    let elsewhere = fx.repo.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let link = fx.repo.join(".worktrees/nergal/link");
    symlink(&elsewhere, &link).unwrap();

    let snapshot = reclaim_snapshot(&fx.repo, &link);
    let got = reclaim_target(&fx.repo, &link, uid(), &snapshot);

    assert_eq!(got.err(), Some(ReclaimIssue::GuardRejected));
}
