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

    let _: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("connecting to {}", socket_path.display()))?;

    stream
        .write_all(input.trim().as_bytes())
        .context("writing to socket")?;
    stream.write_all(b"\n").context("writing newline")?;
    stream.flush().context("flushing socket")?;

    Ok(())
}

/// Checks for pending plan edits and injects a re-read instruction into the prompt.
///
/// If there's a pending edit in HookState, reads stdin JSON, appends a
/// re-read instruction to the message field, and writes modified JSON to stdout.
/// If no pending edit, passes through silently (exit 0).
pub fn inject_edits() -> Result<()> {
    let Some(edit_path) = HookState::take_pending_edit()? else {
        return Ok(());
    };

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading stdin for inject-edits")?;

    let mut json: serde_json::Value =
        serde_json::from_str(input.trim()).context("stdin is not valid JSON")?;

    let instruction = format!(
        "\n\nRe-read the updated plan at {} and incorporate the user's inline edits.",
        edit_path.display()
    );

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
