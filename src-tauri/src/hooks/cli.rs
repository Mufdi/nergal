#![allow(dead_code)]
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::hooks::state::HookState;

/// Send a `{"kind":"control","op":"rescan_agents"}` control message to the
/// hook socket. The running cluihud picks it up and re-scans the registry,
/// updating the available agents view. No-op (best effort) if cluihud isn't
/// running — the connection error is surfaced to the caller.
pub fn send_rescan_agents(socket_path: &Path) -> Result<()> {
    let payload = r#"{"kind":"control","op":"rescan_agents"}"#;
    let mut stream = UnixStream::connect(socket_path)
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

    if let Some(obj) = json.as_object_mut() {
        if let Ok(cluihud_id) = std::env::var("CLUIHUD_SESSION_ID") {
            obj.insert(
                "cluihud_session_id".to_string(),
                serde_json::Value::String(cluihud_id),
            );
        }
    }

    let payload = serde_json::to_string(&json).context("serializing JSON")?;

    let mut stream = UnixStream::connect(socket_path)
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

/// FIFO guard — removes the FIFO on drop.
struct FifoGuard(PathBuf);

impl Drop for FifoGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Synchronous plan review for PermissionRequest[ExitPlanMode] hook.
///
/// Reads the PermissionRequest event from stdin, sends it to the GUI via
/// Unix socket, then blocks on a FIFO until the user approves or denies.
/// Outputs the PermissionRequest decision JSON to stdout.
pub fn plan_review(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for plan-review")?;

    let stdin_json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let cluihud_id = std::env::var("CLUIHUD_SESSION_ID").unwrap_or_default();
    if cluihud_id.is_empty() {
        // No session — allow by default so we don't block Claude
        output_allow()?;
        return Ok(());
    }

    // Create FIFO for blocking communication
    let fifo_path = PathBuf::from(format!("/tmp/cluihud-plan-{}.fifo", std::process::id()));
    if fifo_path.exists() {
        std::fs::remove_file(&fifo_path)?;
    }
    std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .context("creating FIFO")?;
    let _guard = FifoGuard(fifo_path.clone());

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
        "cluihud_session_id": cluihud_id,
    });

    let payload = serde_json::to_string(&socket_msg).context("serializing socket message")?;
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;
    stream
        .write_all(payload.as_bytes())
        .context("writing to socket")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;
    drop(stream);

    // Block reading from FIFO until the GUI writes a decision
    let decision_str = std::fs::read_to_string(&fifo_path).context("reading decision from FIFO")?;

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

    Ok(())
}

/// Synchronous AskUserQuestion interception via GUI.
///
/// Same FIFO pattern as plan_review: reads the PreToolUse[AskUserQuestion]
/// event from stdin, sends it to the GUI, blocks until the user answers,
/// then outputs a PreToolUse response with updatedInput containing the answer.
pub fn ask_user(socket_path: &Path) -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for ask-user")?;

    let stdin_json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let cluihud_id = std::env::var("CLUIHUD_SESSION_ID").unwrap_or_default();
    if cluihud_id.is_empty() {
        // No session — pass through (Claude will ask in terminal)
        return Ok(());
    }

    let fifo_path = PathBuf::from(format!("/tmp/cluihud-ask-{}.fifo", std::process::id()));
    if fifo_path.exists() {
        std::fs::remove_file(&fifo_path)?;
    }
    std::process::Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .context("creating FIFO")?;
    let _guard = FifoGuard(fifo_path.clone());

    let session_id = stdin_json
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tool_input = stdin_json
        .get("tool_input")
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let socket_msg = serde_json::json!({
        "hook_event_name": "AskUser",
        "session_id": session_id,
        "tool_input": tool_input,
        "fifo_path": fifo_path.display().to_string(),
        "cluihud_session_id": cluihud_id,
    });

    let payload = serde_json::to_string(&socket_msg).context("serializing socket message")?;
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;
    stream
        .write_all(payload.as_bytes())
        .context("writing to socket")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;
    drop(stream);

    // Block reading from FIFO until the GUI writes the answers
    let answer_str = std::fs::read_to_string(&fifo_path).context("reading answers from FIFO")?;

    let answer_json: serde_json::Value =
        serde_json::from_str(answer_str.trim()).context("parsing answer JSON")?;

    // Build updatedInput: echo back original questions + add answers map
    let mut updated_input = tool_input;
    if let Some(obj) = updated_input.as_object_mut() {
        if let Some(answers) = answer_json.get("answers") {
            obj.insert("answers".to_string(), answers.clone());
        }
    }

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated_input
        }
    });
    std::io::stdout()
        .write_all(serde_json::to_string(&output)?.as_bytes())
        .context("writing ask-user response to stdout")?;

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
