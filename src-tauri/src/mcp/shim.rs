//! `nergal mcp` — the stdio MCP shim each agent spawns. It is a pure relay:
//! newline-delimited JSON-RPC on stdio ↔ length-framed JSON-RPC on the daemon
//! socket. It injects the cooperative session hint into `initialize` from its
//! own env (inherited `NERGAL_SESSION_ID`, or CC's `CLAUDE_CODE_SESSION_ID`).
//!
//! Degraded mode (daemon unreachable): `initialize` and `tools/list` are
//! answered locally from the SAME daemon-owned source (`initialize_result` /
//! `tool_definitions`), so a degraded reply cannot drift from the real one;
//! `tools/call` returns a structured error. No hang when the daemon is down —
//! the connect attempt fails fast and every message degrades.

use anyhow::Context;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::transport;

/// Entry point for the `nergal mcp` subcommand. Builds a runtime and runs the
/// stdio relay loop until stdin EOF.
pub fn run() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new().context("mcp shim runtime")?;
    rt.block_on(run_async())
}

async fn run_async() -> anyhow::Result<()> {
    let hint = std::env::var("NERGAL_SESSION_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("CLAUDE_CODE_SESSION_ID")
                .ok()
                .filter(|s| !s.is_empty())
        })
        // Fallback: some agents (Codex) spawn MCP servers with a sanitized env,
        // so our own `NERGAL_SESSION_ID` is missing — but an ancestor process
        // (the agent itself, spawned by the PTY with the var set) still has it.
        // Walk the parent chain and recover it. macOS environ readability is
        // restricted by SIP; `None` there is expected and non-fatal.
        .or_else(|| {
            crate::platform_proc::ancestor_env(&["NERGAL_SESSION_ID", "CLAUDE_CODE_SESSION_ID"], 8)
        });

    // Fast, non-hanging connect: a missing/dead socket → degraded mode.
    let mut daemon = transport::connect(&super::socket_path()).await.ok();

    let mut reader = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // stdin EOF — agent closed the shim
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut msg: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // ignore non-JSON noise
        };

        let method = msg
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let is_notification = msg.get("id").is_none();

        if method == "initialize"
            && let Some(h) = &hint
        {
            match msg.get_mut("params").and_then(|p| p.as_object_mut()) {
                Some(obj) => {
                    obj.insert("_nergal_session_hint".into(), json!(h));
                }
                None => msg["params"] = json!({ "_nergal_session_hint": h }),
            }
        }

        // Notifications get no response; relay best-effort, don't await a reply.
        if is_notification {
            if let Some(conn) = daemon.as_mut()
                && let Ok(bytes) = serde_json::to_vec(&msg)
            {
                let _ = transport::write_frame(conn, &bytes).await;
            }
            continue;
        }

        let response = if let Some(conn) = daemon.as_mut() {
            match relay(conn, &msg).await {
                Ok(resp) => resp,
                Err(_) => {
                    // Daemon died mid-session → degrade from here on.
                    daemon = None;
                    degraded_response(&msg)
                }
            }
        } else {
            degraded_response(&msg)
        };

        let mut out = serde_json::to_vec(&response)?;
        out.push(b'\n');
        stdout.write_all(&out).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn relay(conn: &mut tokio::net::UnixStream, msg: &Value) -> anyhow::Result<Value> {
    let bytes = serde_json::to_vec(msg)?;
    transport::write_frame(conn, &bytes).await?;
    let frame = transport::read_frame(conn)
        .await?
        .context("daemon closed connection")?;
    Ok(serde_json::from_slice(&frame)?)
}

/// Build a local response when the daemon is unreachable. `initialize` +
/// `tools/list` mirror the daemon-owned source; everything else is an error.
fn degraded_response(msg: &Value) -> Value {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    match method {
        "initialize" => json!({ "jsonrpc": "2.0", "id": id, "result": super::initialize_result() }),
        "tools/list" => {
            json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": super::tool_definitions() } })
        }
        "tools/call" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32001, "message": "nergal MCP daemon unreachable", "data": { "reason": "daemon_down" } },
        }),
        other => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("method unavailable in degraded mode: {other}") },
        }),
    }
}

/// Extract `key`'s value from a NUL-separated `/proc/<pid>/environ` buffer.
/// Pure helper kept for the unit tests below; runtime env recovery now goes
/// through `platform_proc::ancestor_env` which uses `sysinfo` cross-platform.
#[cfg(test)]
fn find_in_environ(data: &[u8], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    data.split(|&b| b == 0)
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .find_map(|kv| kv.strip_prefix(&prefix).map(str::to_string))
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_in_environ_extracts_value() {
        let buf = b"PATH=/usr/bin\0NERGAL_SESSION_ID=abc-123\0HOME=/home/x\0";
        assert_eq!(
            find_in_environ(buf, "NERGAL_SESSION_ID"),
            Some("abc-123".to_string())
        );
        assert_eq!(find_in_environ(buf, "MISSING"), None);
        // An empty value is treated as absent.
        assert_eq!(
            find_in_environ(b"NERGAL_SESSION_ID=\0", "NERGAL_SESSION_ID"),
            None
        );
    }

    #[test]
    fn degraded_initialize_mirrors_daemon_source() {
        let m = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} });
        let r = degraded_response(&m);
        assert_eq!(
            r["result"]["protocolVersion"],
            super::super::MCP_PROTOCOL_VERSION
        );
    }

    #[test]
    fn degraded_tools_list_matches_registry() {
        let m = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} });
        let r = degraded_response(&m);
        let names: Vec<String> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect();
        // Single source: the degraded list must equal the daemon's registry.
        let expected: Vec<String> = super::super::tool_definitions()
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn degraded_tools_call_is_structured_error() {
        let m = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "whoami" } });
        let r = degraded_response(&m);
        assert_eq!(r["error"]["data"]["reason"], "daemon_down");
    }
}
