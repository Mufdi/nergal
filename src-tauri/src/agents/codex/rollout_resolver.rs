//! Locate the rollout JSONL Codex writes for a session.
//!
//! Codex's session directory layout is `~/.codex/sessions/<yyyy>/<mm>/<dd>/`,
//! and each rollout filename is `rollout-<uuid>.jsonl`. After cluihud
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
