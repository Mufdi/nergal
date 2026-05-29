//! Detached post-session background runner. On SessionEnd / app-close / session
//! or workspace deletion a marker is dropped under `pending-mocs/` and this
//! subcommand is spawned detached to drain them. A global advisory lock keeps a
//! single runner alive; concurrent spawns see the lock and exit 0 immediately.
//!
//! Phase A wires the plumbing (markers, lock, detached spawn, drain loop). The
//! MOC snapshot (#11) + reverse backlinks (N1) slot into `process_marker` in a
//! later phase — the drain/lock/marker lifecycle around them is already final.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

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

    let mut cmd = Command::new("cluihud");
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

/// Drain every pending marker under one global lock. Concurrent invocations see
/// the held lock and exit without work — their markers are covered by the
/// running drain (they were written before the spawn).
pub fn run() -> Result<()> {
    run_in(&pending_dir(), &lock_path())
}

fn run_in(dir: &Path, lock_path: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
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
        return Ok(());
    }

    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match process_marker(&path) {
            Ok(()) => {
                let _ = fs::remove_file(&path);
            }
            Err(e) => {
                // Leave the marker for the next runner to retry.
                tracing::warn!("post-session: marker {} failed: {e}", path.display());
            }
        }
    }
    // Lock releases on drop, but be explicit so intent is unambiguous.
    fs2::FileExt::unlock(&lock_file).ok();
    Ok(())
}

fn process_marker(path: &Path) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    let _marker: Marker =
        serde_json::from_str(&raw).with_context(|| format!("parsing marker {}", path.display()))?;
    // #11 MOC + N1 backlink propagation land here in a later phase; the marker
    // is consumed once that work succeeds.
    Ok(())
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
        run_in(&pending, &lock).unwrap();
        assert!(!pending.join("a.json").exists());
        assert!(!pending.join("b.json").exists());
    }

    #[test]
    fn run_on_missing_dir_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        run_in(&dir.path().join("nope"), &dir.path().join("l.lock")).unwrap();
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
        run_in(&pending, &lock).unwrap();
        assert!(pending.join("a.json").exists());
        fs2::FileExt::unlock(&held).ok();
    }
}
