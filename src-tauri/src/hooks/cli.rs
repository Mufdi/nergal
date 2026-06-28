#![allow(dead_code)]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::hooks::state::HookState;

/// Send a `{"kind":"control","op":"rescan_agents"}` control message to the
/// hook socket. The running nergal picks it up and re-scans the registry,
/// updating the available agents view. No-op (best effort) if nergal isn't
/// running — the connection error is surfaced to the caller.
pub fn send_rescan_agents(socket_path: &Path) -> Result<()> {
    let payload = r#"{"kind":"control","op":"rescan_agents"}"#;
    let mut stream = crate::platform::sync_connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;
    stream
        .write_all(payload.as_bytes())
        .context("writing control message")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;
    Ok(())
}

/// Reads stdin JSON, validates it, sends to the hook socket as a single line.
pub fn send_hook_event(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin")?;

    let mut json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    if let Some(obj) = json.as_object_mut()
        && let Ok(nergal_id) = std::env::var("NERGAL_SESSION_ID")
    {
        obj.insert(
            "nergal_session_id".to_string(),
            serde_json::Value::String(nergal_id),
        );
    }

    let payload = serde_json::to_string(&json).context("serializing JSON")?;

    let mut stream = crate::platform::sync_connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;

    stream
        .write_all(payload.as_bytes())
        .context("writing to socket")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;

    Ok(())
}

/// Checks for pending plan edits and/or annotation feedback and injects
/// instructions into the prompt via stdout JSON mutation.
///
/// Reads HookState once to avoid race conditions between separate take calls.
/// If nothing is pending, passes through silently (exit 0).
pub fn inject_edits() -> Result<()> {
    let mut state = HookState::read()?;
    let edit_path = state.pending_plan_edit.take();
    let annotations = state.pending_annotations.take();

    if edit_path.is_none() && annotations.is_none() {
        return Ok(());
    }

    state.write()?;

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for inject-edits")?;

    let mut json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let mut parts = Vec::new();
    if let Some(path) = edit_path {
        parts.push(format!(
            "Re-read the updated plan at {} and incorporate the user's inline edits.",
            path.display()
        ));
    }
    if let Some(feedback) = annotations {
        parts.push(feedback);
    }
    let instruction = format!("\n\n{}", parts.join("\n\n"));

    if let Some(message) = json.get_mut("message") {
        if let Some(s) = message.as_str() {
            *message = serde_json::Value::String(format!("{s}{instruction}"));
        }
    } else {
        json["message"] = serde_json::Value::String(instruction.trim_start().to_string());
    }

    let output = serde_json::to_string(&json).context("serializing modified JSON")?;
    std::io::stdout()
        .write_all(output.as_bytes())
        .context("writing to stdout")?;

    Ok(())
}

/// FIFO guard — unlinks the FIFO on drop (RAII cleanup).
///
/// Also treats a pre-existing FIFO as hostile: the constructor removes any
/// pre-existing path before use so a stale/pre-seeded decision from a crashed
/// run or a recycled PID cannot be read as a live answer.
struct FifoGuard(PathBuf);

impl FifoGuard {
    /// Create a FIFO at `path`, removing any pre-existing file first.
    ///
    /// WHY remove pre-existing: a stale FIFO from a crashed run could still
    /// hold a forged `allow` written by an attacker who guessed the path.
    /// Unconditional removal-on-entry closes this pre-seeding attack.
    #[cfg(unix)]
    fn create(path: PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| {
                format!("removing hostile pre-existing FIFO {}", path.display())
            })?;
        }
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .context("creating FIFO")?;
        Ok(Self(path))
    }
}

impl Drop for FifoGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Human-scale wall-clock backstop for the plan-review blocking wait.
/// The deliberation can legitimately take minutes; this avoids hanging forever
/// when neither the GUI death detector nor the human resolves the prompt.
const PLAN_REVIEW_WALL_CLOCK_SECS: u64 = 30 * 60; // 30 minutes

/// Liveness poll cadence for the blocking FIFO read.
///
/// WHY ~1s not infinite: a writerless read-only FIFO returns `POLLHUP`
/// continuously on Linux (and with platform-divergent semantics on macOS), so
/// `poll(-1)` would busy-spin or stall the liveness check. An explicit timeout
/// drives the cadence; each tick checks GUI liveness via the pid+starttime
/// token and falls through to deny on detected death.
#[cfg(unix)]
const POLL_TIMEOUT_MS: i32 = 1000;

/// Synchronous plan review for PermissionRequest[ExitPlanMode] hook.
///
/// Reads the PermissionRequest event from stdin, sends it to the GUI via
/// Unix socket, then blocks on a FIFO until the user approves or denies.
/// Outputs the PermissionRequest decision JSON to stdout.
///
/// FIFO hardening (Decision 5):
/// (a) FIFO lives in the per-user 0700 IPC dir — only same-uid can write.
/// (b) Liveness-aware blocking: poll with ~1s timeout + pid+starttime check
///     per tick so GUI death resolves to safe deny without hanging forever.
/// (c) RAII guard unlinks the FIFO on entry (hostile-preexisting) and exit.
/// The full FIFO→PlatformStream unification is sequenced with Windows (where
/// mkfifo does not exist); see design Decision 5 for the deferred roadmap.
pub fn plan_review(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for plan-review")?;

    let stdin_json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let nergal_id = std::env::var("NERGAL_SESSION_ID").unwrap_or_default();
    if nergal_id.is_empty() {
        // No session — allow by default so we don't block Claude
        output_allow()?;
        return Ok(());
    }

    // FIFO lives in the per-user 0700 IPC dir so only a same-uid process
    // can open it for writing — closes the forged-allow approval-gate hole.
    let pid = std::process::id();
    let fifo_path =
        crate::platform::plan_review_fifo_path(pid).context("resolving plan-review FIFO path")?;

    // RAII guard: removes any pre-existing hostile FIFO + unlinks on exit.
    #[cfg(unix)]
    let _guard = FifoGuard::create(fifo_path.clone())?;
    // Windows: the CLI hosts the gate via an owner-only named-pipe server
    // (Decision 6). `sync_listen` (CreateNamedPipeW) MUST complete BEFORE the
    // PlanReview notification is sent so the GUI never races a not-yet-created
    // pipe (Decision 6 ordering invariant, mirroring Unix mkfifo-before-send).
    #[cfg(windows)]
    let mut pipe_server = crate::platform::sync_listen(&fifo_path)?;

    // Build PlanReview event for the socket server
    let session_id = stdin_json
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool_name = stdin_json
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("ExitPlanMode")
        .to_string();
    let tool_input = stdin_json
        .get("tool_input")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let socket_msg = serde_json::json!({
        "hook_event_name": "PlanReview",
        "session_id": session_id,
        "tool_name": tool_name,
        "tool_input": tool_input,
        "fifo_path": fifo_path.display().to_string(),
        "nergal_session_id": nergal_id,
    });

    let payload = serde_json::to_string(&socket_msg).context("serializing socket message")?;
    let mut stream = crate::platform::sync_connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;
    stream
        .write_all(payload.as_bytes())
        .context("writing to socket")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;
    drop(stream);

    // Liveness-aware blocking read from the endpoint.
    // Read the GUI liveness token written at startup by the Nergal GUI.
    // gui_pid_dir() is the same dir the GUI wrote to on both platforms (on
    // Unix it equals the FIFO's parent IPC dir; on Windows the FIFO path is a
    // pipe name with no usable parent, so resolve the token dir explicitly).
    let gui_dir = crate::platform::gui_pid_dir().context("resolving gui.pid dir")?;
    let gui_token = crate::platform::read_gui_pid(&gui_dir);

    #[cfg(unix)]
    {
        let decision_str = blocking_fifo_read_liveness_aware(&fifo_path, gui_token)?;
        let decision: serde_json::Value =
            serde_json::from_str(decision_str.trim()).context("parsing decision JSON")?;

        let approved = decision
            .get("approved")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if approved {
            output_allow()?;
        } else {
            let message = decision
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Plan changes requested");
            output_deny(message)?;
        }
    }
    #[cfg(windows)]
    {
        let decision_str = blocking_pipe_read_liveness_aware(&mut pipe_server, gui_token)?;
        let decision: serde_json::Value =
            serde_json::from_str(decision_str.trim()).context("parsing decision JSON")?;
        let approved = decision
            .get("approved")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if approved {
            output_allow()?;
        } else {
            let message = decision
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Plan changes requested");
            output_deny(message)?;
        }
    }

    Ok(())
}

/// Blocking FIFO read with GUI liveness awareness.
///
/// Opens the FIFO non-blocking (`O_RDONLY | O_NONBLOCK`) and polls with an
/// explicit ~1s timeout per tick. On each timeout:
///   - Checks GUI liveness via the pid+starttime token.
///   - `ESRCH` or start-time mismatch → safe deny (process dead / pid recycled).
///   - GUI alive + human still deciding → keep waiting.
///   - Wall-clock backstop (30 min) → log + safe deny.
///
/// WHY poll() not poll(-1)/POLLHUP: a writerless read-only FIFO returns
/// POLLHUP continuously → busy-spin or stalled liveness check with edge-wait.
/// An explicit timeout budget drives the cadence cleanly.
///
/// WHY not EOF-as-death-signal: the FIFO is connectionless during deliberation
/// (no writer is attached while the human is deciding) so there is no EOF
/// signal to observe. This mechanism is the FIFO-iteration substitute for the
/// connection-close signal that PlatformStream would provide.
#[cfg(unix)]
fn blocking_fifo_read_liveness_aware(
    fifo_path: &Path,
    gui_token: Option<(u32, u64)>,
) -> anyhow::Result<String> {
    use std::os::unix::io::RawFd;

    let path_cstr = std::ffi::CString::new(fifo_path.to_str().unwrap_or_default())
        .context("FIFO path is not a valid C string")?;

    // O_RDONLY | O_NONBLOCK: opens without blocking on the writer side.
    let fd: RawFd = unsafe { libc::open(path_cstr.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error())
            .context("opening FIFO for liveness-aware read");
    }

    // RAII: close the fd on exit
    struct FdGuard(RawFd);
    impl Drop for FdGuard {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }
    let _fd_guard = FdGuard(fd);

    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(PLAN_REVIEW_WALL_CLOCK_SECS);
    let mut buf = Vec::new();

    loop {
        // Check wall-clock backstop before polling
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .unwrap_or_default();
        if remaining.is_zero() {
            tracing::warn!(
                ipc_event = "dead_peer_deny",
                "plan-review wall-clock backstop reached; resolving to safe deny"
            );
            output_deny("Plan review timed out — please resubmit")
                .context("writing wall-clock deny")?;
            // Return a safe-deny sentinel; plan_review() already wrote to stdout
            return Ok(r#"{"approved":false,"message":"wall_clock_backstop"}"#.to_string());
        }

        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        let ret = unsafe { libc::poll(&mut pfd, 1, POLL_TIMEOUT_MS) };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // EINTR — retry
            }
            return Err(err).context("poll on FIFO");
        }

        if ret == 0 || (pfd.revents & libc::POLLIN) == 0 {
            // Timeout tick (or POLLHUP with no data — ignore POLLHUP: a
            // writerless FIFO returns it continuously; it is not a death signal).
            // Check GUI liveness.
            if let Some((pid, start_time)) = gui_token
                && !crate::platform::check_gui_liveness(pid, start_time)
            {
                // GUI dead — safe deny so the agent loop is not hung.
                output_deny("Nergal GUI is no longer running — plan review cancelled")
                    .context("writing dead-GUI deny")?;
                return Ok(r#"{"approved":false,"message":"gui_dead"}"#.to_string());
            }
            // GUI alive (or unknown) — keep waiting
            continue;
        }

        // POLLIN: data available; read it
        let mut chunk = [0u8; 4096];
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr() as *mut libc::c_void, chunk.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                continue;
            }
            return Err(err).context("reading from FIFO");
        }
        if n == 0 {
            // EOF — writer closed; decision is in `buf`
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        // Check for a complete JSON-parseable payload (the GUI writes atomically)
        if serde_json::from_slice::<serde_json::Value>(&buf).is_ok() {
            break;
        }
    }

    String::from_utf8(buf).context("FIFO decision is not UTF-8")
}

/// Windows analog of `blocking_fifo_read_liveness_aware`: the CLI hosts the gate
/// pipe and polls `accept_with_timeout(~1s)`, running the SAME `gui.pid`
/// liveness check + wall-clock backstop between waits (Decision 6). On connect
/// it re-verifies the peer is the same principal (defence-in-depth behind the
/// owner-only SD), then reads the decision JSON the GUI wrote.
#[cfg(windows)]
fn blocking_pipe_read_liveness_aware(
    server: &mut crate::platform::SyncPipeServer,
    gui_token: Option<(u32, u64)>,
) -> anyhow::Result<String> {
    use std::io::Read;
    const TICK_MS: u32 = 1000;
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(PLAN_REVIEW_WALL_CLOCK_SECS);

    loop {
        if std::time::Instant::now() >= deadline {
            tracing::warn!(
                ipc_event = "dead_peer_deny",
                "plan-review wall-clock backstop reached; resolving to safe deny"
            );
            output_deny("Plan review timed out — please resubmit")
                .context("writing wall-clock deny")?;
            return Ok(r#"{"approved":false,"message":"wall_clock_backstop"}"#.to_string());
        }

        match server.accept_with_timeout(TICK_MS)? {
            None => {
                // No client yet — check GUI liveness (mirrors the Unix tick).
                if let Some((pid, start_time)) = gui_token
                    && !crate::platform::check_gui_liveness(pid, start_time)
                {
                    output_deny("Nergal GUI is no longer running — plan review cancelled")
                        .context("writing dead-GUI deny")?;
                    return Ok(r#"{"approved":false,"message":"gui_dead"}"#.to_string());
                }
                // GUI alive (or unknown) — keep waiting.
            }
            Some((mut stream, peer)) => {
                // Same-principal wall (defence-in-depth behind the owner-only SD).
                if !peer.matches_current_process() {
                    tracing::warn!(
                        ipc_event = "peer_rejected",
                        principal = %peer.display(),
                        "plan-review connection from a foreign principal — denying"
                    );
                    output_deny("Plan review rejected — connection from another user")
                        .context("writing foreign-principal deny")?;
                    return Ok(r#"{"approved":false,"message":"foreign_principal"}"#.to_string());
                }
                let mut decision = String::new();
                stream
                    .read_to_string(&mut decision)
                    .context("reading decision from gate pipe")?;
                return Ok(decision);
            }
        }
    }
}

/// Returns immediately so CC's TUI keeps owning the dialog; we only fire a
/// socket message so the GUI can light the session attention indicator.
pub fn notification(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for notification")?;

    let stdin_json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let nergal_id = std::env::var("NERGAL_SESSION_ID").unwrap_or_default();
    if nergal_id.is_empty() {
        return Ok(());
    }

    let session_id = stdin_json
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let notification_type = stdin_json
        .get("notification_type")
        .and_then(|v| v.as_str())
        .map(String::from);
    let message = stdin_json
        .get("message")
        .and_then(|v| v.as_str())
        .map(String::from);

    let socket_msg = serde_json::json!({
        "hook_event_name": "Notification",
        "session_id": session_id,
        "notification_type": notification_type,
        "message": message,
        "nergal_session_id": nergal_id,
    });

    let payload = serde_json::to_string(&socket_msg).context("serializing socket message")?;
    match crate::platform::sync_connect(socket_path) {
        Ok(mut stream) => {
            let _ = stream.write_all(payload.as_bytes());
            let _ = stream.write_all(b"\n");
            let _ = stream.flush();
        }
        Err(e) => {
            tracing::debug!(
                ipc_event = "notifier_connect_failed",
                path = %socket_path.display(),
                error = %e,
                "notification: connect failed (best-effort, GUI may not be running)"
            );
        }
    }

    Ok(())
}

/// AskUserQuestion notifier (non-blocking).
///
/// CC's TUI handles the prompt natively; nergal only fires a socket message
/// so the GUI can highlight the session tab needing attention. Returns
/// immediately with empty stdout so CC proceeds.
pub fn ask_user(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for ask-user")?;

    let stdin_json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let nergal_id = std::env::var("NERGAL_SESSION_ID").unwrap_or_default();
    if nergal_id.is_empty() {
        return Ok(());
    }

    let session_id = stdin_json
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let socket_msg = serde_json::json!({
        "hook_event_name": "AskUser",
        "session_id": session_id,
        "tool_input": serde_json::Value::Object(serde_json::Map::new()),
        "nergal_session_id": nergal_id,
    });

    let payload = serde_json::to_string(&socket_msg).context("serializing socket message")?;
    match crate::platform::sync_connect(socket_path) {
        Ok(mut stream) => {
            let _ = stream.write_all(payload.as_bytes());
            let _ = stream.write_all(b"\n");
            let _ = stream.flush();
        }
        Err(e) => {
            tracing::debug!(
                ipc_event = "notifier_connect_failed",
                path = %socket_path.display(),
                error = %e,
                "ask-user: connect failed (best-effort, GUI may not be running)"
            );
        }
    }

    Ok(())
}

fn output_allow() -> Result<()> {
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {
                "behavior": "allow"
            }
        }
    });
    std::io::stdout()
        .write_all(serde_json::to_string(&output)?.as_bytes())
        .context("writing allow decision to stdout")?;
    Ok(())
}

fn output_deny(message: &str) -> Result<()> {
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {
                "behavior": "deny",
                "message": message
            }
        }
    });
    std::io::stdout()
        .write_all(serde_json::to_string(&output)?.as_bytes())
        .context("writing deny decision to stdout")?;
    Ok(())
}
