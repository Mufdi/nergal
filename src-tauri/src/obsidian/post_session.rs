//! Detached post-session background runner. On SessionEnd / app-close / session
//! or workspace deletion a marker is dropped under `pending-mocs/` and this
//! subcommand is spawned detached to drain them. A global advisory lock keeps a
//! single runner alive; concurrent spawns see the lock and exit 0 immediately.
//!
//! Phase A wires the plumbing (markers, lock, detached spawn, drain loop). The
//! MOC snapshot (#11) + reverse backlinks (N1) slot into `process_marker` in a
//! later phase — the drain/lock/marker lifecycle around them is already final.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

/// Detached-spawn capability, probed once at startup. Default optimistic so the
/// very first finalize (before the probe lands) still attempts the bg path.
static RUNNER_HEALTHY: AtomicBool = AtomicBool::new(true);

/// Whether bg session-snapshot processing is available on this host. False on
/// hardened distros where the probe found the detached child cannot survive.
pub fn runner_available() -> bool {
    RUNNER_HEALTHY.load(Ordering::Relaxed)
}

fn finalized_sessions() -> &'static Mutex<HashSet<String>> {
    static F: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Returns true the first time finalization is claimed for `session_id`, false
/// after. The SessionEnd hook and the PTY-EOF trigger can both fire for one
/// session; the log footer + marker must run exactly once.
pub fn claim_finalization(session_id: &str) -> bool {
    finalized_sessions()
        .lock()
        .map(|mut s| s.insert(session_id.to_string()))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub session_id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub trigger: String,
    /// Unix epoch milliseconds. Drives the >10min stale-marker recovery scan.
    pub created_at: u64,
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().expect("home dir").join(".config"))
        .join("cluihud")
}

fn pending_dir() -> PathBuf {
    config_dir().join("pending-mocs")
}

fn lock_path() -> PathBuf {
    config_dir().join("post-session.lock")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Atomic marker write (tmp + rename) so a half-written marker is never drained.
pub fn write_marker(
    session_id: &str,
    workspace_id: &str,
    agent_id: &str,
    trigger: &str,
) -> Result<()> {
    write_marker_in(&pending_dir(), session_id, workspace_id, agent_id, trigger)
}

fn write_marker_in(
    dir: &Path,
    session_id: &str,
    workspace_id: &str,
    agent_id: &str,
    trigger: &str,
) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let marker = Marker {
        session_id: session_id.to_string(),
        workspace_id: workspace_id.to_string(),
        agent_id: agent_id.to_string(),
        trigger: trigger.to_string(),
        created_at: now_ms(),
    };
    let json = serde_json::to_string(&marker)?;
    let final_path = dir.join(format!("{session_id}.json"));
    let tmp = dir.join(format!("{session_id}.json.tmp"));
    fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &final_path)
        .with_context(|| format!("renaming into {}", final_path.display()))?;
    Ok(())
}

/// Spawn `cluihud post-session` detached so it outlives the GUI process.
#[cfg(target_os = "linux")]
pub fn spawn_runner_detached() -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    // Re-exec the running binary (dev or installed), not a PATH lookup that
    // could hit a stale `cluihud` lacking the post-session subcommand.
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cluihud"));
    let mut cmd = Command::new(exe);
    cmd.arg("post-session")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs in the forked child before exec. setsid detaches
    // it from the parent's session + controlling terminal so the GUI exit does
    // not kill it. No heap allocation occurs between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()
        .context("spawning detached cluihud post-session")?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_runner_detached() -> Result<()> {
    Ok(())
}

/// Probe whether detached bg processing survives on this host. Some hardened
/// distros seccomp-kill children of sandboxed desktop apps; detecting it once
/// lets finalize paths fall back to inline MOC builds instead of stranding
/// snapshots in markers no runner will ever drain. The probe doubles as the
/// first real drain — it invokes the same `post-session` subcommand. Sets
/// `RUNNER_HEALTHY` and returns the result.
#[cfg(target_os = "linux")]
pub fn probe_spawn_health() -> bool {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("cluihud"));
    let mut cmd = Command::new(exe);
    cmd.arg("post-session")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: see spawn_runner_detached — setsid in the forked child, no alloc.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            RUNNER_HEALTHY.store(false, Ordering::Relaxed);
            return false;
        }
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    let healthy = match child.try_wait() {
        // Clean early exit (drained + released the lock) → spawning works.
        Ok(Some(status)) => status.success(),
        // Still draining after 200ms → spawning works; it is doing real work.
        Ok(None) => true,
        Err(_) => false,
    };
    RUNNER_HEALTHY.store(healthy, Ordering::Relaxed);
    healthy
}

#[cfg(not(target_os = "linux"))]
pub fn probe_spawn_health() -> bool {
    true
}

/// Drain every pending marker under one global lock. Concurrent invocations see
/// the held lock and exit without work — their markers are covered by the
/// running drain (they were written before the spawn).
pub fn run() -> Result<()> {
    let db = crate::db::Database::open().context("opening database for post-session runner")?;
    let n = drain(&pending_dir(), &lock_path(), &|path| {
        process_marker(path, &db)
    })?;
    // A trailing INFO line is the success sentinel the next launch checks for —
    // its absence (a dangling ERROR as the last line) surfaces a failure toast.
    log_line(&format!("INFO drained {n} markers"));
    Ok(())
}

/// Synchronous drain used when the startup probe found bg spawning unavailable.
/// Builds MOCs inline against the GUI's own database connection so snapshots are
/// not lost on hardened distros.
pub fn drain_inline(db: &crate::db::Database) -> Result<()> {
    let n = drain(&pending_dir(), &lock_path(), &|path| {
        process_marker(path, db)
    })?;
    log_line(&format!("INFO drained {n} markers (inline)"));
    Ok(())
}

/// Lock-guarded drain loop. The per-marker work is injected so the lock/marker
/// lifecycle stays testable without a database. Returns the count of markers
/// processed + removed.
fn drain(dir: &Path, lock_path: &Path, process: &dyn Fn(&Path) -> Result<()>) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("opening lock {}", lock_path.display()))?;
    if lock_file.try_lock_exclusive().is_err() {
        return Ok(0);
    }

    let mut drained = 0;
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match process(&path) {
            Ok(()) => {
                let _ = fs::remove_file(&path);
                drained += 1;
            }
            Err(e) => {
                // Leave the marker for the next runner to retry; stderr is nulled
                // in the detached process, so failures go to the log file.
                log_line(&format!("ERROR marker {}: {e:#}", path.display()));
            }
        }
    }
    // Lock releases on drop, but be explicit so intent is unambiguous.
    fs2::FileExt::unlock(&lock_file).ok();
    Ok(drained)
}

fn process_marker(path: &Path, db: &crate::db::Database) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    let marker: Marker =
        serde_json::from_str(&raw).with_context(|| format!("parsing marker {}", path.display()))?;
    // Session gone (deleted before the runner reached it) → nothing to snapshot;
    // drop the stale marker rather than retrying it forever.
    if db.find_session(&marker.session_id)?.is_none() {
        return Ok(());
    }
    let cfg =
        crate::obsidian::config::resolve(&marker.workspace_id, |w| db.get_obsidian_config(w))?;
    if let Some(moc_path) = crate::obsidian::moc::MocBuilder::build(&marker.session_id, &cfg, db)? {
        let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc_path, &cfg);
    }
    Ok(())
}

/// What the startup recovery pass found, so the frontend can surface the right
/// toast once it has mounted.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct StartupReport {
    /// Stale markers (> threshold) found before recovery ran.
    pub recovered: usize,
    /// The previous runner's last log line was an ERROR (incomplete drain).
    pub last_run_failed: bool,
    /// The spawn probe found bg processing unavailable on this host.
    pub bg_unavailable: bool,
    /// Absolute path to the runner log, for the failure toast.
    pub log_path: String,
}

/// On launch: probe spawn capability (which doubles as the first drain), and if
/// bg processing is unavailable drain pending markers inline so nothing is lost.
/// Returns a report driving the recovery/failure toasts.
pub fn startup_recover(db: &crate::db::Database, stale_after_ms: u64) -> StartupReport {
    let recovered = count_stale_in(&pending_dir(), stale_after_ms, now_ms());
    let last_run_failed = last_run_failed();
    let healthy = probe_spawn_health();
    if !healthy {
        let _ = drain_inline(db);
    }
    StartupReport {
        recovered,
        last_run_failed,
        bg_unavailable: !healthy,
        log_path: log_file_path().to_string_lossy().into_owned(),
    }
}

fn count_stale_in(dir: &Path, stale_after_ms: u64, now: u64) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut stale = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(m) = serde_json::from_str::<Marker>(&raw)
            && now.saturating_sub(m.created_at) > stale_after_ms
        {
            stale += 1;
        }
    }
    stale
}

fn log_file_path() -> PathBuf {
    config_dir().join("logs").join("post-session.log")
}

/// Cap on the runner log before it rolls to a numbered generation.
const MAX_RUNNER_LOG_BYTES: u64 = 5 * 1024 * 1024;
/// Generations retained beyond the active log (`.1`, `.2`, `.3`).
const RUNNER_LOG_GENERATIONS: u32 = 3;

fn gen_path(path: &Path, n: u32) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{n}"));
    PathBuf::from(s)
}

/// Roll `post-session.log` → `.1` → `.2` → `.3`, dropping the oldest, once the
/// active file exceeds the cap. Best-effort: a failed rotation just keeps appending.
fn rotate_runner_log(path: &Path) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() <= MAX_RUNNER_LOG_BYTES {
        return;
    }
    let _ = fs::remove_file(gen_path(path, RUNNER_LOG_GENERATIONS));
    for n in (1..RUNNER_LOG_GENERATIONS).rev() {
        let _ = fs::rename(gen_path(path, n), gen_path(path, n + 1));
    }
    let _ = fs::rename(path, gen_path(path, 1));
}

/// Append-only runner log. The detached process has nulled stdio, so this is the
/// only place its failures surface (the app tails it on next launch).
fn log_line(msg: &str) {
    let dir = config_dir().join("logs");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("post-session.log");
    rotate_runner_log(&path);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{} {msg}", now_ms());
    }
}

/// True if the runner log's last non-empty line is an ERROR — a previous drain
/// died mid-flight. Drives the "last snapshot failed" toast on next launch.
fn last_run_failed() -> bool {
    let Ok(content) = fs::read_to_string(log_file_path()) else {
        return false;
    };
    content
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.contains("ERROR"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        write_marker_in(dir.path(), "sid-1", "ws-1", "claude-code", "SessionEnd").unwrap();
        let raw = fs::read_to_string(dir.path().join("sid-1.json")).unwrap();
        let m: Marker = serde_json::from_str(&raw).unwrap();
        assert_eq!(m.session_id, "sid-1");
        assert_eq!(m.workspace_id, "ws-1");
        assert_eq!(m.trigger, "SessionEnd");
        assert!(m.created_at > 0);
        // No leftover tmp file.
        assert!(!dir.path().join("sid-1.json.tmp").exists());
    }

    #[test]
    fn run_drains_markers_and_deletes_them() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending-mocs");
        let lock = dir.path().join("post-session.lock");
        write_marker_in(&pending, "a", "ws", "claude-code", "SessionEnd").unwrap();
        write_marker_in(&pending, "b", "ws", "claude-code", "app-close").unwrap();
        let n = drain(&pending, &lock, &|_| Ok(())).unwrap();
        assert_eq!(n, 2);
        assert!(!pending.join("a.json").exists());
        assert!(!pending.join("b.json").exists());
    }

    #[test]
    fn drain_count_excludes_failed_markers() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending-mocs");
        let lock = dir.path().join("post-session.lock");
        write_marker_in(&pending, "ok", "ws", "cc", "SessionEnd").unwrap();
        write_marker_in(&pending, "bad", "ws", "cc", "SessionEnd").unwrap();
        let n = drain(&pending, &lock, &|p| {
            if p.file_stem().and_then(|s| s.to_str()) == Some("bad") {
                anyhow::bail!("boom");
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 1);
        // Failed marker survives for retry; succeeded one is gone.
        assert!(pending.join("bad.json").exists());
        assert!(!pending.join("ok.json").exists());
    }

    #[test]
    fn claim_finalization_is_once_per_session() {
        let sid = "claim-test-session-unique";
        assert!(claim_finalization(sid));
        assert!(!claim_finalization(sid));
    }

    #[test]
    fn gen_path_appends_generation_suffix() {
        let p = Path::new("/tmp/logs/post-session.log");
        assert_eq!(gen_path(p, 1), Path::new("/tmp/logs/post-session.log.1"));
        assert_eq!(gen_path(p, 3), Path::new("/tmp/logs/post-session.log.3"));
    }

    #[test]
    fn counts_only_markers_older_than_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        // created_at is now_ms() at write time; with now far in the future, all
        // markers read as stale. With now == write time, none are.
        write_marker_in(p, "a", "ws", "cc", "SessionEnd").unwrap();
        write_marker_in(p, "b", "ws", "cc", "app-close").unwrap();
        assert_eq!(count_stale_in(p, 600_000, now_ms() + 10_000_000), 2);
        assert_eq!(count_stale_in(p, 600_000, 0), 0);
    }

    #[test]
    fn run_on_missing_dir_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        drain(
            &dir.path().join("nope"),
            &dir.path().join("l.lock"),
            &|_| Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn second_runner_skips_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending-mocs");
        let lock = dir.path().join("post-session.lock");
        write_marker_in(&pending, "a", "ws", "claude-code", "SessionEnd").unwrap();
        // Hold the lock, then a concurrent run_in must no-op (marker survives).
        fs::create_dir_all(&pending).unwrap();
        let held = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock)
            .unwrap();
        held.try_lock_exclusive().unwrap();
        drain(&pending, &lock, &|_| Ok(())).unwrap();
        assert!(pending.join("a.json").exists());
        fs2::FileExt::unlock(&held).ok();
    }
}
