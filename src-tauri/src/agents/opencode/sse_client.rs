//! Minimal SSE consumer for the OpenCode TUI's embedded Hono server.
//!
//! The TUI runs the same HTTP server as `opencode serve`; spawning with
//! `--port <X>` pins it to a known address. This module connects to
//! `http://127.0.0.1:<X>/event`, listens for the events cluihud cares about
//! (tool/file-edit lifecycle), and forwards them as [`HookEvent`]s onto the
//! adapter event sink. Translated events flow through the same dispatcher
//! that handles CC's socket hooks, so panels (Modified Files, Activity)
//! light up identically.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::agents::EventSink;
use crate::hooks::events::HookEvent;

/// Shared map populated by the SSE consumer when it observes a
/// `session.created` / `session.updated` event. Keyed by the cluihud session
/// id so the adapter can look up the OpenCode-side id for resume.
pub type SessionIdMap = Arc<DashMap<String, String>>;

/// Handle for a running SSE consumer task. Drop or call [`Self::cancel`].
pub struct SseClient {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl SseClient {
    pub async fn cancel(mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
        }
    }

    /// Start the consumer. Polls the port for readiness up to 8s, then
    /// subscribes to `/event`. Logs and exits on terminal errors; the TUI
    /// stays usable in the terminal independently of this task.
    pub fn spawn(
        base_url: String,
        cluihud_session_id: String,
        sink: EventSink,
        session_ids: SessionIdMap,
    ) -> Self {
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let join = tokio::spawn(async move {
            // Poll-connect for readiness — the TUI's Hono server starts a
            // few hundred ms after the binary launches.
            let event_url = format!("{base_url}/event");
            // SSE streams are long-lived; an overall request timeout would
            // close the body mid-stream and surface as `error decoding
            // response body`. Bound only the connect phase.
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("reqwest client builder");

            let resp = match wait_for_sse(&client, &event_url, &mut cancel_rx).await {
                Some(r) => r,
                None => return,
            };

            let mut stream = resp.bytes_stream().eventsource();
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => break,
                    next = stream.next() => {
                        let Some(item) = next else {
                            tracing::debug!(session = %cluihud_session_id, "opencode SSE stream ended");
                            break;
                        };
                        let event = match item {
                            Ok(ev) => ev,
                            Err(e) => {
                                // Break instead of `continue`: stream errors
                                // tend to be terminal (TUI exited, server
                                // closed) and looping spams the log without
                                // recovering. The next session creation
                                // will spawn a fresh client.
                                tracing::warn!(
                                    session = %cluihud_session_id,
                                    error = %e,
                                    "opencode SSE stream error; ending consumer",
                                );
                                break;
                            }
                        };
                        // Capture OpenCode's session id from the first
                        // `session.created` / `session.updated` event so the
                        // adapter can persist it for resume via `--session <id>`.
                        if let Some(oc_id) = extract_session_id(&event.data) {
                            session_ids
                                .entry(cluihud_session_id.clone())
                                .or_insert(oc_id);
                        }
                        if let Some(hook_event) = translate_event(&cluihud_session_id, &event.data)
                            && sink.send(hook_event).is_err() {
                                // Receiver dropped — runtime is shutting down.
                                break;
                            }
                    }
                }
            }
        });

        Self {
            cancel: Some(cancel_tx),
            join: Some(join),
        }
    }
}

impl Drop for SseClient {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            h.abort();
        }
    }
}

/// Probe the `/event` endpoint until a 200 is returned or the cancel token
/// fires. Returns `None` when cancelled or the server never came up.
async fn wait_for_sse(
    client: &reqwest::Client,
    url: &str,
    cancel: &mut oneshot::Receiver<()>,
) -> Option<reqwest::Response> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if cancel.try_recv().is_ok() {
            return None;
        }
        match client.get(url).send().await {
            Ok(r) if r.status().is_success() => return Some(r),
            Ok(_) | Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    tracing::warn!(url, "opencode SSE never became ready");
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

/// SSE payload — only the discriminator + properties we care about. Unknown
/// shapes are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCodeEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    properties: Value,
}

/// Pull the OpenCode-side session id out of `session.created` /
/// `session.updated` events. Returns `None` for any other event type or a
/// missing id field.
fn extract_session_id(raw: &str) -> Option<String> {
    let ev: OpenCodeEvent = serde_json::from_str(raw).ok()?;
    if !matches!(
        ev.event_type.as_str(),
        "session.created" | "session.updated"
    ) {
        return None;
    }
    // Prefer `info.id` (the canonical session payload); fall back to
    // `sessionID` for older shapes.
    ev.properties
        .pointer("/info/id")
        .and_then(|v| v.as_str())
        .or_else(|| ev.properties.get("sessionID").and_then(|v| v.as_str()))
        .map(String::from)
}

/// Translate an OpenCode SSE event into a [`HookEvent`] the dispatcher
/// already understands. Returns `None` for events we don't surface yet.
///
/// Event reference (sst/opencode @ dev, `bus/bus-event.ts` + publish sites):
/// - `file.edited` → `{ file: string }` — fires once per Write/Edit tool.
/// - `message.part.updated` → `{ sessionID, part: Part, time }` where
///   `part.type == "tool"` and `part.state.status == "completed"` exposes
///   `part.tool` (tool name) + `part.state.input` (args, may contain
///   `filePath`).
fn translate_event(cluihud_session_id: &str, raw: &str) -> Option<HookEvent> {
    let ev: OpenCodeEvent = serde_json::from_str(raw).ok()?;
    match ev.event_type.as_str() {
        // Direct file-modification signal — the cleanest source for the
        // Modified Files panel. Synthesise a PostToolUse so the existing
        // dispatcher path (`process_file_event`) handles it without new code.
        "file.edited" => {
            let path = ev.properties.get("file").and_then(|v| v.as_str())?;
            Some(HookEvent::PostToolUse {
                session_id: cluihud_session_id.into(),
                tool_name: "Edit".into(),
                tool_input: serde_json::json!({ "file_path": path }),
                tool_result: None,
            })
        }
        // Tool lifecycle — fires for every tool, including non-file ones
        // (read, grep, bash). We only surface completed tool calls so the
        // activity log has the canonical shape (`PostToolUse`).
        "message.part.updated" => {
            let part = ev.properties.get("part")?;
            if part.get("type").and_then(|v| v.as_str()) != Some("tool") {
                return None;
            }
            let state = part.get("state")?;
            if state.get("status").and_then(|v| v.as_str()) != Some("completed") {
                return None;
            }
            let tool_name = part
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let input = state.get("input").cloned().unwrap_or(Value::Null);
            Some(HookEvent::PostToolUse {
                session_id: cluihud_session_id.into(),
                tool_name,
                tool_input: input,
                tool_result: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_file_edited_emits_synthetic_post_tool_use() {
        let raw = r#"{"type":"file.edited","properties":{"file":"/tmp/y.rs"}}"#;
        let ev = translate_event("ses-1", raw).expect("expected Some");
        match ev {
            HookEvent::PostToolUse {
                tool_name,
                tool_input,
                ..
            } => {
                assert_eq!(tool_name, "Edit");
                assert_eq!(
                    tool_input.get("file_path").and_then(|v| v.as_str()),
                    Some("/tmp/y.rs")
                );
            }
            other => panic!("expected PostToolUse, got {other:?}"),
        }
    }

    #[test]
    fn translate_message_part_updated_completed_tool_emits_post_tool_use() {
        let raw = r#"{
            "type":"message.part.updated",
            "properties":{
                "sessionID":"ses",
                "part":{
                    "type":"tool",
                    "tool":"Write",
                    "state":{
                        "status":"completed",
                        "input":{"filePath":"/tmp/foo.rs","content":"x"}
                    }
                }
            }
        }"#;
        let ev = translate_event("ses-1", raw).expect("expected Some");
        match ev {
            HookEvent::PostToolUse {
                tool_name,
                tool_input,
                ..
            } => {
                assert_eq!(tool_name, "Write");
                assert_eq!(
                    tool_input.get("filePath").and_then(|v| v.as_str()),
                    Some("/tmp/foo.rs")
                );
            }
            other => panic!("expected PostToolUse, got {other:?}"),
        }
    }

    #[test]
    fn translate_message_part_updated_pending_tool_returns_none() {
        // Only `completed` should surface — running/pending must not emit.
        let raw = r#"{
            "type":"message.part.updated",
            "properties":{
                "part":{"type":"tool","tool":"Edit","state":{"status":"running"}}
            }
        }"#;
        assert!(translate_event("ses-1", raw).is_none());
    }

    #[test]
    fn translate_message_part_updated_text_part_returns_none() {
        let raw = r#"{
            "type":"message.part.updated",
            "properties":{"part":{"type":"text","content":"hi"}}
        }"#;
        assert!(translate_event("ses-1", raw).is_none());
    }

    #[test]
    fn translate_unknown_event_returns_none() {
        let raw = r#"{"type":"server.connected","properties":{}}"#;
        assert!(translate_event("ses-1", raw).is_none());
    }

    #[test]
    fn translate_malformed_returns_none() {
        assert!(translate_event("ses-1", "not json").is_none());
    }
}
