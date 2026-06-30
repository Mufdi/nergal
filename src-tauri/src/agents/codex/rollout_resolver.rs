//! Locate the rollout JSONL Codex writes for a session.
//!
//! Codex's session directory layout is `~/.codex/sessions/<yyyy>/<mm>/<dd>/`,
//! and each rollout filename is `rollout-<uuid>.jsonl`. After nergal
//! spawns Codex, the rollout file appears within seconds; we poll for the
//! newest `rollout-*.jsonl` whose mtime is after the spawn timestamp.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, anyhow};

/// Wait up to `timeout` for a `rollout-*.jsonl` file modified after `after`
/// to appear under `sessions_root`. Returns the absolute path on success.
pub async fn find_rollout_after_spawn(
    sessions_root: &Path,
    after: SystemTime,
    timeout: Duration,
) -> Result<PathBuf> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(p) = scan_for_newest_rollout(sessions_root, after).await {
            return Ok(p);
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(anyhow!(
        "no rollout-*.jsonl appeared under {} after spawn timestamp within {:?}",
        sessions_root.display(),
        timeout
    ))
}

/// Wait up to `timeout` for the `rollout-*.jsonl` whose `session_meta.cwd`
/// matches `cwd` (newest such, when several share a cwd). More reliable than
/// newest-after-spawn: another live codex session — or a resumed one reusing an
/// older file — won't be mistaken for this session's rollout.
pub async fn find_rollout_for_cwd(
    sessions_root: &Path,
    cwd: &str,
    timeout: Duration,
) -> Result<PathBuf> {
    let start = Instant::now();
    loop {
        let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
        collect_rollouts(sessions_root, &mut candidates).await;
        candidates.sort_by_key(|c| std::cmp::Reverse(c.1));
        for (path, _) in &candidates {
            if rollout_cwd_matches(path, cwd).await {
                return Ok(path.clone());
            }
        }
        if start.elapsed() >= timeout {
            return Err(anyhow!(
                "no rollout with cwd {cwd} under {} within {:?}",
                sessions_root.display(),
                timeout
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Read a rollout's first line (the `session_meta` record) and compare its
/// `payload.cwd` to `cwd`.
async fn rollout_cwd_matches(path: &Path, cwd: &str) -> bool {
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return false;
    };
    let Some(first) = content.lines().next() else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(first) else {
        return false;
    };
    v.get("payload")
        .and_then(|p| p.get("cwd"))
        .and_then(|c| c.as_str())
        == Some(cwd)
}

async fn collect_rollouts(root: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
    if !root.exists() {
        return;
    }
    Box::pin(collect_walk(root, out)).await;
}

async fn collect_walk(dir: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        if meta.is_dir() {
            Box::pin(collect_walk(&path, out)).await;
        } else if meta.is_file() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                out.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
            }
        }
    }
}

/// Pull the UUID out of a rollout filename. `rollout-abc123.jsonl` → `Some("abc123")`.
/// Returns `None` if the filename doesn't match the documented shape.
pub fn extract_uuid_from_filename(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    stem.strip_prefix("rollout-").map(String::from)
}

async fn scan_for_newest_rollout(root: &Path, after: SystemTime) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }
    // Walk up to three levels deep (yyyy/mm/dd). A full recursive walk would
    // be wasteful — Codex's layout is fixed.
    let mut latest: Option<(PathBuf, SystemTime)> = None;
    walk_three_levels(root, &mut latest, after).await;
    latest.map(|(p, _)| p)
}

async fn walk_three_levels(
    dir: &Path,
    best: &mut Option<(PathBuf, SystemTime)>,
    after: SystemTime,
) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            // recurse, but avoid pathological deep traversal — three levels
            // (yyyy/mm/dd) covers Codex's layout. Box::pin is required for
            // recursive async fn.
            Box::pin(walk_three_levels(&path, best, after)).await;
        } else if meta.is_file() {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if mtime < after {
                continue;
            }
            if best.as_ref().is_none_or(|(_, t)| mtime > *t) {
                *best = Some((path, mtime));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_uuid_from_filename_handles_canonical_shape() {
        assert_eq!(
            extract_uuid_from_filename(Path::new("/x/y/rollout-abc123.jsonl")).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn extract_uuid_from_filename_returns_none_for_unrelated_files() {
        assert!(extract_uuid_from_filename(Path::new("foo.txt")).is_none());
        assert!(extract_uuid_from_filename(Path::new("rollout.jsonl")).is_none());
        assert!(extract_uuid_from_filename(Path::new("/x/.jsonl")).is_none());
    }
}
