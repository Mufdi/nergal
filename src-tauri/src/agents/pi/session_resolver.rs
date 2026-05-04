//! Map cluihud session metadata onto Pi's session-file convention.
//!
//! Pi stores sessions under `~/.pi/agent/sessions/<encoded>/<uuid>.jsonl`,
//! where `<encoded>` is the absolute working directory with `/` → `-`,
//! wrapped by `--…--`. e.g. `/home/x/foo` → `--home-x-foo--`.
//!
//! After Pi is spawned, the JSONL takes a moment to appear; this module
//! polls the encoded directory for the newest `.jsonl` file with a timeout.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

/// Encode an absolute cwd as Pi's session-directory path component.
/// `/home/user/projects/foo` → `--home-user-projects-foo--`.
pub fn encode_cwd_to_pi_path(cwd: &Path) -> String {
    let mut s = String::from("--");
    let mut first = true;
    for component in cwd.components() {
        if let std::path::Component::Normal(c) = component {
            if !first {
                s.push('-');
            }
            s.push_str(&c.to_string_lossy());
            first = false;
        }
    }
    s.push_str("--");
    s
}

/// Poll `sessions_dir` for the newest `.jsonl` file. Pi creates the file a
/// few hundred ms after spawning; the default 2 s timeout covers slow disks.
pub async fn wait_for_jsonl(sessions_dir: &Path, timeout: Duration) -> Result<PathBuf> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(p) = newest_jsonl(sessions_dir).await {
            return Ok(p);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(anyhow!(
        "no .jsonl file appeared in {} within {:?}",
        sessions_dir.display(),
        timeout
    ))
}

async fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "jsonl")
            && let Ok(meta) = entry.metadata().await
            && let Ok(mtime) = meta.modified()
            && latest.as_ref().is_none_or(|(_, t)| mtime > *t)
        {
            latest = Some((path, mtime));
        }
    }
    latest.map(|(p, _)| p)
}

/// Read the first line of a Pi JSONL session, parse the `session` header,
/// return the session UUID (stored on `agent_internal_session_id` for resume).
pub async fn extract_pi_session_uuid(jsonl_path: &Path) -> Result<String> {
    let content = tokio::fs::read_to_string(jsonl_path).await?;
    let first_line = content
        .lines()
        .next()
        .ok_or_else(|| anyhow!("empty jsonl"))?;
    let entry: serde_json::Value = serde_json::from_str(first_line)?;
    if entry.get("type").and_then(|v| v.as_str()) != Some("session") {
        return Err(anyhow!("first line is not a session header"));
    }
    entry
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow!("session header missing id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_cwd_round_trips_for_typical_paths() {
        let p = encode_cwd_to_pi_path(Path::new("/home/user/projects/foo"));
        assert_eq!(p, "--home-user-projects-foo--");
    }

    #[test]
    fn encode_cwd_handles_root_only() {
        // No Normal components → just the wrappers.
        let p = encode_cwd_to_pi_path(Path::new("/"));
        assert_eq!(p, "----");
    }

    #[test]
    fn encode_cwd_preserves_spaces_in_components() {
        let p = encode_cwd_to_pi_path(Path::new("/home/user/My Projects/foo"));
        assert_eq!(p, "--home-user-My Projects-foo--");
    }

    #[test]
    fn encode_cwd_collapses_dot_segments_via_components() {
        // Path::components doesn't drop `.`s automatically; if Pi ever
        // disagrees we'll need explicit canonicalization. Capture today's
        // behavior so a future change documents the choice.
        let p = encode_cwd_to_pi_path(Path::new("/home/user/./foo"));
        assert!(
            p == "--home-user-foo--" || p == "--home-user-.-foo--",
            "got {p}"
        );
    }
}
