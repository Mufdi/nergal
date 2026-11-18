use std::collections::HashSet;
use std::ffi::{CString, OsString};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::{ReclaimFailure, ReclaimIssue, git, permission_remedy, remove_worktree_plain};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const RECLAIM_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const OUTPUT_HEAD_BYTES: usize = 4096;
const READER_JOIN_LIMIT: Duration = Duration::from_secs(5);

pub(crate) const RECLAIM_SCRIPT: &str = "[ \"$(stat -c %d:%i /w)\" = \"$1\" ] || exit 3; exec find /w -xdev -type d -exec chown -h \"$2\" {} + -exec chmod u+rwx {} +";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimTarget {
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
}

impl ReclaimIssue {
    pub fn reason(&self) -> &'static str {
        match self {
            ReclaimIssue::GuardRejected => "guard rejected",
            ReclaimIssue::InProgress => "removal in progress",
            ReclaimIssue::DockerNotFound => "docker not found",
            ReclaimIssue::DockerUnsupported => "docker unsupported",
            ReclaimIssue::DockerFailed => "docker failed",
            ReclaimIssue::Ineffective => "reclaim ineffective",
        }
    }
}

pub fn registered_worktrees(repo_path: &Path) -> Vec<PathBuf> {
    let Ok(output) = git()
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .filter_map(|path| Path::new(path).canonicalize().ok())
        .collect()
}

pub fn reclaim_snapshot(repo_path: &Path, recorded: &Path) -> Vec<PathBuf> {
    let mut snapshot = registered_worktrees(repo_path);
    if let Ok(canonical) = recorded.canonicalize()
        && !snapshot.contains(&canonical)
    {
        snapshot.push(canonical);
    }
    snapshot
}

pub fn reclaim_target(
    repo_path: &Path,
    worktree_path: &Path,
    uid: u32,
    registered: &[PathBuf],
) -> Result<ReclaimTarget, ReclaimIssue> {
    let reject = ReclaimIssue::GuardRejected;
    let canonical_repo = repo_path.canonicalize().map_err(|_| reject)?;

    let canonical = worktree_path.canonicalize().map_err(|_| reject)?;
    let nergal_dir = canonical_repo.join(".worktrees").join("nergal");
    let slug = canonical.strip_prefix(&nergal_dir).map_err(|_| reject)?;
    let mut components = slug.components();
    let (Some(Component::Normal(slug_name)), None) = (components.next(), components.next()) else {
        return Err(reject);
    };

    let mut slug_meta = None;
    for dir in [
        canonical_repo.join(".worktrees"),
        nergal_dir.clone(),
        nergal_dir.join(slug_name),
    ] {
        let meta = std::fs::symlink_metadata(&dir).map_err(|_| reject)?;
        if !meta.file_type().is_dir() {
            return Err(reject);
        }
        slug_meta = Some(meta);
    }

    let meta = slug_meta.ok_or(reject)?;
    if meta.uid() != uid {
        return Err(reject);
    }
    if canonical
        .as_os_str()
        .as_bytes()
        .iter()
        .any(|b| matches!(b, b',' | b'"' | b':' | b'\n'))
    {
        return Err(reject);
    }
    if !registered.contains(&canonical) {
        return Err(reject);
    }
    Ok(ReclaimTarget {
        path: canonical,
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

pub fn blocked_dirs(dir: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if !meta.file_type().is_dir() {
        return false;
    }
    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
        return true;
    };
    if unsafe { libc::access(c_path.as_ptr(), libc::W_OK | libc::X_OK) } != 0 {
        return true;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => return e.kind() == std::io::ErrorKind::PermissionDenied,
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .any(|entry| blocked_dirs(&entry.path()))
}

pub fn unblock_own_dirs(dir: &Path, uid: u32) {
    let Ok(c_path) = CString::new(dir.as_os_str().as_bytes()) else {
        return;
    };
    // O_PATH works on a 0000 directory the user owns, and O_NOFOLLOW makes a
    // symlink swapped in after the caller's lstat fail instead of being chmod-ed.
    let raw = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_PATH | libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if raw < 0 {
        return;
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0
        || st.st_mode & libc::S_IFMT != libc::S_IFDIR
    {
        return;
    }
    let via_fd = PathBuf::from(format!("/proc/self/fd/{}", fd.as_raw_fd()));
    if st.st_uid == uid {
        let mode = (st.st_mode & 0o1755) | 0o700;
        let _ = std::fs::set_permissions(&via_fd, std::fs::Permissions::from_mode(mode));
    }
    let Ok(entries) = std::fs::read_dir(&via_fd) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            unblock_own_dirs(&dir.join(entry.file_name()), uid);
        }
    }
}

pub(crate) fn docker_unblock_args(
    name: &str,
    endpoint: &str,
    target: &ReclaimTarget,
    uid: u32,
    gid: u32,
) -> Vec<OsString> {
    let mut mount = OsString::from("type=bind,source=");
    mount.push(target.path.as_os_str());
    mount.push(",target=/w");
    let mut args: Vec<OsString> = [
        "-H",
        endpoint,
        "run",
        "--rm",
        "--name",
        name,
        "--pull",
        "never",
        "--network",
        "none",
        "--user",
        "0:0",
        "--entrypoint",
        "sh",
        "--mount",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(mount);
    args.extend(
        [
            "alpine:3",
            "-c",
            RECLAIM_SCRIPT,
            "reclaim",
            &format!("{}:{}", target.dev, target.ino),
            &format!("{uid}:{gid}"),
        ]
        .iter()
        .map(OsString::from),
    );
    args
}

pub(crate) fn docker_supported(
    version_output: &str,
    security_options_json: &str,
    endpoint: &str,
) -> bool {
    !version_output.to_lowercase().contains("podman")
        && !security_options_json.contains("rootless")
        && !security_options_json.contains("userns")
        && endpoint.trim().starts_with("unix://")
}

pub trait Reclaimer {
    fn unblock_own(&self, dir: &Path, uid: u32) {
        unblock_own_dirs(dir, uid)
    }
    fn docker_status(&self) -> Result<(), ReclaimIssue>;
    fn docker_unblock(&self, target: &ReclaimTarget, uid: u32, gid: u32) -> bool;
}

pub fn reclaim_ownership(
    r: &dyn Reclaimer,
    target: &ReclaimTarget,
    uid: u32,
    gid: u32,
) -> Result<(), ReclaimIssue> {
    r.docker_status()?;
    if !r.docker_unblock(target, uid, gid) {
        return Err(ReclaimIssue::DockerFailed);
    }
    if blocked_dirs(&target.path) {
        return Err(ReclaimIssue::Ineffective);
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClaimError {
    InProgress,
    Gone,
}

fn claims() -> &'static Mutex<HashSet<PathBuf>> {
    static CLAIMS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    CLAIMS.get_or_init(|| Mutex::new(HashSet::new()))
}

#[derive(Debug)]
pub struct RemovalClaim(PathBuf);

impl Drop for RemovalClaim {
    fn drop(&mut self) {
        claims()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

pub fn claim_removal(path: &Path) -> Result<RemovalClaim, ClaimError> {
    let canonical = path.canonicalize().map_err(|_| ClaimError::Gone)?;
    let mut held = claims().lock().unwrap_or_else(|e| e.into_inner());
    if !held.insert(canonical.clone()) {
        return Err(ClaimError::InProgress);
    }
    Ok(RemovalClaim(canonical))
}

struct Finished {
    status: Option<ExitStatus>,
    stdout: String,
    stderr_head: String,
}

fn spawn_head_reader<R: Read + Send + 'static>(mut source: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut head = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = source.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = OUTPUT_HEAD_BYTES.saturating_sub(head.len());
            head.extend_from_slice(&buf[..n.min(room)]);
        }
        String::from_utf8_lossy(&head).into_owned()
    })
}

fn join_bounded(handle: std::thread::JoinHandle<String>) -> String {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(handle.join().unwrap_or_default());
    });
    rx.recv_timeout(READER_JOIN_LIMIT).unwrap_or_default()
}

fn kill_group(pid: u32) {
    if let Ok(pgid) = i32::try_from(pid) {
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
    }
}

fn run_limited(
    program: &Path,
    args: &[OsString],
    limit: Duration,
    capture_stdout: bool,
    on_expiry: impl FnOnce(),
) -> std::io::Result<Finished> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(if capture_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let stdout_reader = child.stdout.take().map(spawn_head_reader);
    let stderr_reader = child.stderr.take().map(spawn_head_reader);

    let deadline = Instant::now() + limit;
    let mut on_expiry = Some(on_expiry);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(e) => {
                kill_group(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        }
        if Instant::now() >= deadline {
            if let Some(expire) = on_expiry.take() {
                expire();
            }
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    Ok(Finished {
        status,
        stdout: stdout_reader.map(join_bounded).unwrap_or_default(),
        stderr_head: stderr_reader.map(join_bounded).unwrap_or_default(),
    })
}

pub struct DockerReclaimer {
    docker: Option<PathBuf>,
    status: OnceLock<Result<String, ReclaimIssue>>,
}

impl Default for DockerReclaimer {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerReclaimer {
    pub fn new() -> Self {
        Self {
            docker: which::which("docker").ok(),
            status: OnceLock::new(),
        }
    }

    fn probe(docker: &Path, args: &[&str]) -> Result<String, ReclaimIssue> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        match run_limited(docker, &args, PROBE_TIMEOUT, true, || {}) {
            Ok(Finished {
                status: Some(status),
                stdout,
                ..
            }) if status.success() => Ok(stdout),
            Ok(finished) => {
                tracing::warn!(
                    args = ?args,
                    status = ?finished.status,
                    stderr = %finished.stderr_head,
                    "docker probe failed"
                );
                Err(ReclaimIssue::DockerFailed)
            }
            Err(e) => {
                tracing::warn!(args = ?args, error = %e, "docker probe could not run");
                Err(ReclaimIssue::DockerFailed)
            }
        }
    }

    fn check(&self) -> Result<String, ReclaimIssue> {
        let docker = self.docker.as_deref().ok_or(ReclaimIssue::DockerNotFound)?;
        let version = Self::probe(docker, &["--version"])?;
        if version.to_lowercase().contains("podman") {
            return Err(ReclaimIssue::DockerUnsupported);
        }
        let options = Self::probe(docker, &["info", "--format", "{{json .SecurityOptions}}"])?;
        let endpoint = Self::probe(
            docker,
            &[
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ],
        )?;
        if docker_supported(&version, &options, &endpoint) {
            Ok(endpoint.trim().to_string())
        } else {
            Err(ReclaimIssue::DockerUnsupported)
        }
    }
}

impl Reclaimer for DockerReclaimer {
    fn docker_status(&self) -> Result<(), ReclaimIssue> {
        self.status
            .get_or_init(|| self.check())
            .as_ref()
            .map(|_| ())
            .map_err(|issue| *issue)
    }

    fn docker_unblock(&self, target: &ReclaimTarget, uid: u32, gid: u32) -> bool {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let (Some(docker), Some(Ok(endpoint))) = (self.docker.as_deref(), self.status.get()) else {
            return false;
        };
        let name = format!(
            "nergal-reclaim-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let args = docker_unblock_args(&name, endpoint, target, uid, gid);
        let remove_container = || {
            let rm_args: Vec<OsString> = ["-H", endpoint.as_str(), "rm", "-f", name.as_str()]
                .iter()
                .map(OsString::from)
                .collect();
            if let Err(e) = run_limited(docker, &rm_args, PROBE_TIMEOUT, false, || {}) {
                tracing::warn!(container = %name, error = %e, "docker rm -f failed");
            }
        };
        match run_limited(docker, &args, RECLAIM_TIMEOUT, false, remove_container) {
            Ok(finished) => {
                let ok = finished.status.is_some_and(|s| s.success());
                if !ok {
                    tracing::warn!(
                        dir = %target.path.display(),
                        status = ?finished.status,
                        stderr = %finished.stderr_head,
                        "docker reclaim did not finish cleanly"
                    );
                }
                ok
            }
            Err(e) => {
                tracing::warn!(dir = %target.path.display(), error = %e, "docker reclaim could not run");
                false
            }
        }
    }
}

fn remove_dir(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)
    } else {
        Ok(())
    }
}

fn prune(repo_path: &Path) -> Result<std::process::Output> {
    git()
        .args(["worktree", "prune"])
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree prune")
}

fn finish_ok(repo_path: &Path, remove_stderr: &str) -> Result<()> {
    let prune = prune(repo_path)?;
    if !prune.status.success() {
        let prune_err = String::from_utf8_lossy(&prune.stderr);
        anyhow::bail!(
            "git worktree remove failed ({remove_stderr}); prune also failed: {prune_err}"
        );
    }
    Ok(())
}

fn reclaim_failure(issue: ReclaimIssue, message: String) -> anyhow::Error {
    anyhow::Error::new(ReclaimFailure { issue, message })
}

fn fail(
    repo_path: &Path,
    worktree_path: &Path,
    err: &std::io::Error,
    issue: Option<ReclaimIssue>,
) -> Result<()> {
    let _ = prune(repo_path);
    let Some(issue) = issue else {
        let remedy = permission_remedy(err, worktree_path);
        anyhow::bail!(
            "could not remove worktree dir {}: {err}{remedy}",
            worktree_path.display()
        );
    };
    let (remedy, hint) = if issue == ReclaimIssue::Ineffective {
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        (
            permission_remedy(&denied, worktree_path),
            "; stop any container that mounts this worktree and try again",
        )
    } else {
        (permission_remedy(err, worktree_path), "")
    };
    Err(reclaim_failure(
        issue,
        format!(
            "could not remove worktree dir {}: {err}{remedy} (reclaim: {}){hint}",
            worktree_path.display(),
            issue.reason()
        ),
    ))
}

#[cfg(test)]
pub fn remove_worktree_with(
    repo_path: &Path,
    worktree_path: &Path,
    reclaimer: Option<&dyn Reclaimer>,
) -> Result<()> {
    remove_worktree_snapshotting(repo_path, worktree_path, reclaimer, |repo, _| {
        registered_worktrees(repo)
    })
}

pub(crate) fn remove_worktree_snapshotting(
    repo_path: &Path,
    worktree_path: &Path,
    reclaimer: Option<&dyn Reclaimer>,
    snapshot_of: fn(&Path, &Path) -> Vec<PathBuf>,
) -> Result<()> {
    let Some(r) = reclaimer else {
        return remove_worktree_plain(repo_path, worktree_path);
    };
    let snapshot = snapshot_of(repo_path, worktree_path);
    if !worktree_path.exists() {
        return remove_worktree_plain(repo_path, worktree_path);
    }
    let _claim = match claim_removal(worktree_path) {
        Ok(claim) => claim,
        Err(ClaimError::Gone) => return remove_worktree_plain(repo_path, worktree_path),
        Err(ClaimError::InProgress) => {
            return Err(reclaim_failure(
                ReclaimIssue::InProgress,
                format!(
                    "could not remove worktree dir {}: {}",
                    worktree_path.display(),
                    ReclaimIssue::InProgress.reason()
                ),
            ));
        }
    };

    let output = git()
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree remove")?;
    if output.status.success() {
        return Ok(());
    }
    let remove_stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let mut err = match remove_dir(worktree_path) {
        Ok(()) => return finish_ok(repo_path, &remove_stderr),
        Err(e) => e,
    };
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        r.unblock_own(worktree_path, uid);
        match remove_dir(worktree_path) {
            Ok(()) => return finish_ok(repo_path, &remove_stderr),
            Err(e) => err = e,
        }
    }
    if err.kind() != std::io::ErrorKind::PermissionDenied {
        return fail(repo_path, worktree_path, &err, None);
    }

    let target = match reclaim_target(repo_path, worktree_path, uid, &snapshot) {
        Ok(target) => target,
        Err(issue) => return fail(repo_path, worktree_path, &err, Some(issue)),
    };
    if let Err(issue) = reclaim_ownership(r, &target, uid, gid) {
        return fail(repo_path, worktree_path, &err, Some(issue));
    }
    match remove_dir(worktree_path) {
        Ok(()) => finish_ok(repo_path, &remove_stderr),
        Err(e) => fail(
            repo_path,
            worktree_path,
            &e,
            Some(ReclaimIssue::Ineffective),
        ),
    }
}
