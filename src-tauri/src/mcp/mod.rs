//! cluihud MCP daemon: exposes the live session directory to the agents
//! cluihud wraps, over a dedicated length-framed Unix socket.
//!
//! Design (see `openspec/changes/cluihud-mcp-server/design.md`):
//! - Decision 1: a NEW bidirectional socket, not the fire-and-forget hook socket.
//! - Decision 2: the **uid** is the only access boundary; identity is the
//!   cooperative env hint the shim reports, validated against the live registry.
//! - Decision 2b: the directory is global-read within the uid (no per-caller gate).
//! - Decision 3: snapshot-then-release; no blocking I/O under a lock (in `directory`).
//! - Decision 8: `mcp_server_enabled` defaults off; `tools/call` → `mcp_disabled`.

pub mod directory;
pub mod registration;
pub mod shim;
pub mod summary;
pub mod transport;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::agents::state::AgentRuntimeState;
use crate::config::Config;
use crate::db::SharedDb;

/// MCP protocol revision advertised by `initialize`. Shared by the daemon and
/// the shim's degraded-mode reply so they cannot drift.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// JSON-RPC error codes: the standard band plus one cluihud server error.
const PARSE_ERROR: i64 = -32700;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
/// Server is reachable but disabled by the user (default-off posture).
const MCP_DISABLED: i64 = -32001;

/// Dedicated MCP socket path (per-user temp dir). NOT the hook socket.
pub fn socket_path() -> PathBuf {
    std::env::temp_dir().join("cluihud-mcp.sock")
}

/// Toggle the MCP server: persist the flag and (de)register the shim in agent
/// MCP configs. The daemon itself always binds; this gates `tools/call` and
/// controls whether agents have the shim wired at all.
#[tauri::command]
pub fn mcp_set_enabled(enabled: bool) -> Result<(), String> {
    let mut config = Config::load();
    config.mcp_server_enabled = enabled;
    config.save().map_err(|e| format!("{e:#}"))?;
    let res = if enabled {
        registration::register()
    } else {
        registration::deregister()
    };
    res.map_err(|e| format!("{e:#}"))
}

/// Everything a tool body needs. Cheap to clone (inner `Arc`s).
#[derive(Clone)]
pub struct DaemonContext {
    pub db: SharedDb,
    pub agents: AgentRuntimeState,
    /// The app process uid; connections from any other uid are rejected.
    pub app_uid: u32,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

fn ok(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn err(id: Option<Value>, code: i64, message: &str, data: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data,
        }),
    }
}

/// Wrap a tool's JSON output in the MCP `tools/call` content envelope.
fn tool_ok(id: Option<Value>, value: Value) -> JsonRpcResponse {
    ok(
        id,
        json!({
            "content": [{ "type": "text", "text": value.to_string() }],
            "isError": false,
        }),
    )
}

/// The MCP `initialize` result — the single source the shim mirrors in degraded
/// mode so a daemon-down `initialize` cannot drift from the real one.
pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "cluihud", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The tool registry — the daemon owns it; the shim vendors this exact list for
/// degraded `tools/list` (single source, no drift).
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "whoami",
            "description": "Identify the calling agent's own cluihud session (or report unidentified).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        }),
        json!({
            "name": "list_sessions",
            "description": "List all live cluihud sessions across every workspace (excludes the caller's own session unless include_self).",
            "inputSchema": {
                "type": "object",
                "properties": { "include_self": { "type": "boolean", "description": "Include the caller's own session (default false)." } },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "get_session",
            "description": "Full descriptor for one live cluihud session by id.",
            "inputSchema": {
                "type": "object",
                "properties": { "session_id": { "type": "string" } },
                "required": ["session_id"],
                "additionalProperties": false,
            },
        }),
    ]
}

/// Pure JSON-RPC dispatch. `identity` is the caller's resolved cluihud session
/// id (or `None` = unidentified). `enabled` is resolved by the caller from
/// config per request so this stays pure and unit-testable.
pub fn dispatch(
    ctx: &DaemonContext,
    identity: Option<&str>,
    enabled: bool,
    req: &JsonRpcRequest,
) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => ok(req.id.clone(), initialize_result()),
        "tools/list" => ok(req.id.clone(), json!({ "tools": tool_definitions() })),
        "tools/call" => {
            if !enabled {
                return err(
                    req.id.clone(),
                    MCP_DISABLED,
                    "cluihud MCP server is disabled",
                    Some(
                        json!({ "reason": "mcp_disabled", "hint": "enable it in cluihud Settings → MCP" }),
                    ),
                );
            }
            let name = req.params.get("name").and_then(|v| v.as_str());
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match name {
                Some("whoami") => tool_ok(req.id.clone(), directory::whoami(ctx, identity)),
                Some("list_sessions") => {
                    let include_self = args
                        .get("include_self")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let exclude = if include_self { None } else { identity };
                    match directory::list_sessions(ctx, exclude) {
                        Ok(list) => tool_ok(req.id.clone(), json!(list)),
                        Err(e) => err(req.id.clone(), INTERNAL_ERROR, &e.to_string(), None),
                    }
                }
                Some("get_session") => match args.get("session_id").and_then(|v| v.as_str()) {
                    Some(id) => match directory::get_session(ctx, id) {
                        Ok(Some(d)) => tool_ok(req.id.clone(), json!(d)),
                        Ok(None) => err(
                            req.id.clone(),
                            INVALID_PARAMS,
                            "no live session with that id",
                            None,
                        ),
                        Err(e) => err(req.id.clone(), INTERNAL_ERROR, &e.to_string(), None),
                    },
                    None => err(
                        req.id.clone(),
                        INVALID_PARAMS,
                        "session_id is required",
                        None,
                    ),
                },
                Some(other) => err(
                    req.id.clone(),
                    METHOD_NOT_FOUND,
                    &format!("unknown tool: {other}"),
                    None,
                ),
                None => err(
                    req.id.clone(),
                    INVALID_PARAMS,
                    "tool name is required",
                    None,
                ),
            }
        }
        other => err(
            req.id.clone(),
            METHOD_NOT_FOUND,
            &format!("unknown method: {other}"),
            None,
        ),
    }
}

/// Daemon accept loop. Binds the socket, enforces the uid wall, and spawns a
/// per-connection task. Returns only on a fatal accept error.
///
/// A semaphore caps concurrent connections so a runaway same-uid spawner can't
/// pin unbounded tasks/heap (each connection can allocate up to MAX_FRAME_BYTES
/// per read). The threat is same-uid (already full-trust), so the cap is a
/// resource backstop, not an authz boundary.
pub async fn serve(transport: transport::UnixSocketTransport, ctx: DaemonContext) {
    const MAX_CONNECTIONS: usize = 32;
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    tracing::info!("mcp daemon listening on {}", transport.path().display());
    loop {
        match transport.accept().await {
            Ok((stream, uid)) => {
                if uid != ctx.app_uid {
                    tracing::warn!(
                        "mcp: rejected connection from uid {uid} (app uid {})",
                        ctx.app_uid
                    );
                    continue; // stream dropped → closed
                }
                let Ok(permit) = sem.clone().try_acquire_owned() else {
                    tracing::warn!("mcp: connection limit reached, dropping connection");
                    continue; // stream dropped → closed
                };
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let _permit = permit; // released on task end
                    handle_connection(stream, ctx).await
                });
            }
            Err(e) => {
                tracing::error!("mcp accept error: {e}");
                break;
            }
        }
    }
}

async fn handle_connection(mut stream: tokio::net::UnixStream, ctx: DaemonContext) {
    // Per-connection cooperative identity hint, captured from `initialize`.
    // Re-resolved against the live registry on every tool call (lazy
    // re-validation handles the connect-before-register race).
    let mut hint: Option<String> = None;
    loop {
        let frame = match transport::read_frame(&mut stream).await {
            Ok(Some(f)) => f,
            Ok(None) => break, // clean peer close
            Err(e) => {
                tracing::debug!("mcp connection read error: {e}");
                break;
            }
        };
        let req: JsonRpcRequest = match serde_json::from_slice(&frame) {
            Ok(r) => r,
            Err(e) => {
                let resp = err(None, PARSE_ERROR, &format!("invalid JSON-RPC: {e}"), None);
                if write_response(&mut stream, &resp).await.is_err() {
                    break;
                }
                continue;
            }
        };
        // Latch the hint on the FIRST initialize that carries it — a later
        // initialize can't silently re-identify the connection.
        if hint.is_none()
            && req.method == "initialize"
            && let Some(h) = req
                .params
                .get("_cluihud_session_hint")
                .and_then(|v| v.as_str())
        {
            hint = Some(h.to_string());
        }
        // JSON-RPC notifications (no id) get no response.
        let is_notification = req.id.is_none();
        let identity = hint
            .as_deref()
            .and_then(|h| ctx.agents.resolve_session_hint(h));
        let enabled = Config::load().mcp_server_enabled;
        let resp = dispatch(&ctx, identity.as_deref(), enabled, &req);
        if is_notification {
            continue;
        }
        if write_response(&mut stream, &resp).await.is_err() {
            break;
        }
    }
}

async fn write_response(
    stream: &mut tokio::net::UnixStream,
    resp: &JsonRpcResponse,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(resp).unwrap_or_else(|_| {
        br#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialize failed"}}"#
            .to_vec()
    });
    transport::write_frame(stream, &bytes).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::{Arc, Mutex};

    fn test_ctx() -> DaemonContext {
        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let agents = AgentRuntimeState::bootstrap().unwrap();
        DaemonContext {
            db,
            agents,
            app_uid: 0,
        }
    }

    fn req(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            id: Some(json!(1)),
            method: method.to_string(),
            params,
        }
    }

    #[test]
    fn initialize_reports_protocol_and_tools_capability() {
        let ctx = test_ctx();
        let r = dispatch(&ctx, None, true, &req("initialize", json!({})));
        let result = r.result.unwrap();
        assert_eq!(result["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_returns_the_three_tools() {
        let ctx = test_ctx();
        let r = dispatch(&ctx, None, true, &req("tools/list", json!({})));
        let tools = r.result.unwrap()["tools"].as_array().unwrap().clone();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, vec!["whoami", "list_sessions", "get_session"]);
    }

    #[test]
    fn tools_call_disabled_returns_mcp_disabled() {
        let ctx = test_ctx();
        let r = dispatch(
            &ctx,
            None,
            false,
            &req("tools/call", json!({ "name": "whoami", "arguments": {} })),
        );
        let e = r.error.unwrap();
        assert_eq!(e.code, MCP_DISABLED);
        assert_eq!(e.data.unwrap()["reason"], "mcp_disabled");
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let ctx = test_ctx();
        let r = dispatch(&ctx, None, true, &req("frobnicate", json!({})));
        assert_eq!(r.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn unknown_tool_is_method_not_found() {
        let ctx = test_ctx();
        let r = dispatch(
            &ctx,
            None,
            true,
            &req("tools/call", json!({ "name": "nope", "arguments": {} })),
        );
        assert_eq!(r.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn get_session_without_id_is_invalid_params() {
        let ctx = test_ctx();
        let r = dispatch(
            &ctx,
            None,
            true,
            &req(
                "tools/call",
                json!({ "name": "get_session", "arguments": {} }),
            ),
        );
        assert_eq!(r.error.unwrap().code, INVALID_PARAMS);
    }

    #[test]
    fn list_sessions_empty_when_no_live_sessions() {
        let ctx = test_ctx();
        let r = dispatch(
            &ctx,
            None,
            true,
            &req(
                "tools/call",
                json!({ "name": "list_sessions", "arguments": {} }),
            ),
        );
        // tool_ok wraps as content text; parse it back.
        let text = r.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        let list: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 0);
    }

    #[test]
    fn whoami_unidentified_when_no_hint() {
        let ctx = test_ctx();
        let r = dispatch(
            &ctx,
            None,
            true,
            &req("tools/call", json!({ "name": "whoami", "arguments": {} })),
        );
        let text = r.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        let who: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(who["identified"], false);
    }
}
