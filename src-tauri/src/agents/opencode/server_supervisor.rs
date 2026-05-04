//! Manages the lifecycle of one `opencode serve` child process per cluihud
//! session.
//!
//! Why per-session: `opencode serve` is a stateful process that owns one
//! agent session at a time. Sharing the server across cluihud sessions would
//! conflate transcripts and permission queues. The cost is one extra process
//! per session, which is acceptable at personal-use scale.
//!
//! Lifecycle:
//! 1. [`ServerSupervisor::start`] spawns `opencode serve --port 0`. The flag
//!    `0` asks the server to bind any free ephemeral port; we capture the
//!    actual port from its stdout banner ("opencode server listening on
//!    http://127.0.0.1:<port>"). The PID is recorded to a state file so a
//!    crashed cluihud can clean up its orphans on next launch.
//! 2. [`ServerSupervisor::stop`] sends SIGTERM, waits up to 5 s, removes
//!    the PID file.
//! 3. [`ServerSupervisor::cleanup_orphans`] runs at app startup: any PID
//!    file whose parent cluihud is no longer alive gets the corresponding
//!    child force-killed.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// One running `opencode serve` instance. The `Child` is held to keep the
/// process alive; dropping it sends SIGKILL on Linux.
pub struct ServerInstance {
    pub child: Child,
    pub port: u16,
    pub started_at: Instant,
    pub pid_file: PathBuf,
}

/// Per-process supervisor. `instances` is keyed by cluihud session id.
pub struct ServerSupervisor {
    instances: DashMap<String, ServerInstance>,
}

impl Default for ServerSupervisor {
    fn default() -> Self {
        Self {
            instances: DashMap::new(),
        }
    }
}

impl ServerSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn `opencode serve --port 0` for `session_id`. Returns the port the
    /// server is listening on. Returns the existing port if already started
    /// for this session (idempotent — useful when start_event_pump re-fires).
    pub async fn start(&self, session_id: &str, binary: &Path) -> Result<u16> {
        if let Some(inst) = self.instances.get(session_id) {
            return Ok(inst.port);
        }

        let mut cmd = Command::new(binary);
        cmd.arg("serve")
            .arg("--port")
            .arg("0")
            .arg("--hostname")
            .arg("127.0.0.1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // Tag the server's process group so cleanup is unambiguous.
            .env("CLUIHUD_SESSION_ID", session_id);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning opencode serve for session {session_id}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("opencode serve produced no stdout"))?;
        let port = parse_port_from_stdout(stdout).await?;

        let pid_dir = pid_dir();
        tokio::fs::create_dir_all(&pid_dir).await.ok();
        let pid_file = pid_dir.join(format!("{session_id}.pid"));
        let parent_pid = std::process::id();
        let child_pid = child.id().unwrap_or(0);
        let _ = tokio::fs::write(
            &pid_file,
            format!("parent={parent_pid}\nchild={child_pid}\nport={port}\n"),
        )
        .await;

        self.instances.insert(
            session_id.to_string(),
            ServerInstance {
                child,
                port,
                started_at: Instant::now(),
                pid_file,
            },
        );
        Ok(port)
    }

    /// SIGTERM the child; wait up to 5 s for it to exit; clean up PID file.
    pub async fn stop(&self, session_id: &str) -> Result<()> {
        if let Some((_, mut inst)) = self.instances.remove(session_id) {
            let _ = inst.child.start_kill();
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(5), inst.child.wait()).await;
            let _ = tokio::fs::remove_file(&inst.pid_file).await;
        }
        Ok(())
    }

    /// Lookup the port a session is currently bound to, if any.
    pub fn port_for(&self, session_id: &str) -> Option<u16> {
        self.instances.get(session_id).map(|i| i.port)
    }

    /// Walk the PID directory; for any PID file whose parent cluihud
    /// (recorded as `parent=`) is no longer alive, kill the orphaned child
    /// and remove the file. Linux-only (uses `/proc/<pid>`).
    pub async fn cleanup_orphans() {
        let pid_dir = pid_dir();
        let mut entries = match tokio::fs::read_dir(&pid_dir).await {
            Ok(e) => e,
            Err(_) => return,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let parent_pid = parse_kv_u32(&content, "parent=");
            let child_pid = parse_kv_u32(&content, "child=");
            if parent_pid.is_none_or(|p| !proc_alive(p))
                && let Some(cpid) = child_pid
            {
                tracing::info!(
                    pid_file = %path.display(),
                    child_pid = cpid,
                    "killing orphan opencode serve",
                );
                let _ = Command::new("kill")
                    .arg("-9")
                    .arg(cpid.to_string())
                    .status()
                    .await;
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }
}

fn pid_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("cluihud")
        .join("opencode-pids")
}

fn parse_kv_u32(content: &str, key: &str) -> Option<u32> {
    content
        .lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.strip_prefix(key))
        .and_then(|s| s.trim().parse().ok())
}

#[cfg(target_os = "linux")]
fn proc_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn proc_alive(_pid: u32) -> bool {
    // Outside Linux we conservatively treat the parent as alive so we don't
    // kill a server that might still be in use. Cluihud is Linux-first.
    true
}

/// Read child stdout line-by-line, looking for the banner
/// `opencode server listening on http://127.0.0.1:<port>`. Times out after
/// 10 s — if we don't see the banner the child is misbehaving.
async fn parse_port_from_stdout<R: tokio::io::AsyncRead + Unpin + Send + 'static>(
    stdout: R,
) -> Result<u16> {
    let port = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .context("reading opencode stdout")?;
            if n == 0 {
                return Err(anyhow!("opencode stdout closed before banner"));
            }
            if let Some(p) = extract_port(&line) {
                // Spawn a task to drain the rest of stdout so the pipe doesn't
                // back-pressure the child. We don't need the content but we
                // can't drop the reader without closing the pipe.
                tokio::spawn(async move {
                    let mut sink = String::new();
                    while reader.read_line(&mut sink).await.unwrap_or(0) > 0 {
                        sink.clear();
                    }
                });
                return Ok(p);
            }
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for opencode 'listening on' banner"))??;
    Ok(port)
}

/// Pull the port from a banner like
/// `opencode server listening on http://127.0.0.1:14096`.
/// Tolerant: `0.0.0.0`, IPv6, missing scheme, ANSI-coloured lines all work
/// because we just look for the last `:NNNN` token before whitespace/EOL.
fn extract_port(line: &str) -> Option<u16> {
    let lower = line.to_lowercase();
    if !lower.contains("listening on") {
        return None;
    }
    // Grab the substring from the last colon onward, then read digits.
    let colon = line.rfind(':')?;
    let tail = &line[colon + 1..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_port_parses_canonical_banner() {
        let line = "opencode server listening on http://127.0.0.1:14096\n";
        assert_eq!(extract_port(line), Some(14096));
    }

    #[test]
    fn extract_port_ignores_unrelated_lines() {
        assert_eq!(extract_port("INFO: starting up"), None);
        assert_eq!(extract_port(""), None);
    }

    #[test]
    fn extract_port_handles_ipv6_localhost() {
        let line = "listening on http://[::1]:14096";
        assert_eq!(extract_port(line), Some(14096));
    }

    #[test]
    fn parse_kv_u32_finds_keyed_lines() {
        let body = "parent=12345\nchild=67890\nport=14096\n";
        assert_eq!(parse_kv_u32(body, "parent="), Some(12345));
        assert_eq!(parse_kv_u32(body, "child="), Some(67890));
        assert_eq!(parse_kv_u32(body, "missing="), None);
    }
}
