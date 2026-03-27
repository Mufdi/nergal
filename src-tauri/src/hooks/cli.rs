#![allow(dead_code)]
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result};

use crate::hooks::state::HookState;

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
