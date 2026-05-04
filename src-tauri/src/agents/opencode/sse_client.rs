//! SSE event consumer for `opencode serve`.
//!
//! Connects to `GET /event`, frames events with `eventsource-stream`,
//! translates them into the runtime's `EventSink`, and stashes pending
//! permission requests for later REST replies. Reconnects with backoff on
//! transport failure — the lifetime of the SSE client matches the
//! `start_event_pump` / `stop_event_pump` calls.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use dashmap::DashMap;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::permission_client::PendingPermission;
use crate::agents::EventSink;
use crate::hooks::events::HookEvent;

/// Handle to a running SSE consumer task. Drop the handle to cancel; or call
/// [`SseClient::cancel`] to be explicit.
pub struct SseClient {
    handle: Option<JoinHandle<()>>,
    cancel: Option<oneshot::Sender<()>>,
}

impl SseClient {
    /// Start a background task that consumes `<base_url>/event` and emits
    /// translated events to `sink`. Returns immediately.
    pub fn spawn(
        base_url: String,
        cluihud_session_id: String,
        port: u16,
        sink: EventSink,
        pending: Arc<DashMap<String, PendingPermission>>,
    ) -> Self {
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let url = format!("{base_url}/event");
            let mut backoff = Duration::from_millis(250);
            let max_backoff = Duration::from_secs(10);

            loop {
                let connect = tokio::select! {
                    _ = &mut cancel_rx => return,
                    res = connect_and_stream(&url, &cluihud_session_id, port, &sink, &pending) => res,
                };
                match connect {
                    Ok(()) => {
                        tracing::debug!(session = %cluihud_session_id, "opencode SSE stream ended cleanly; reconnecting");
                        backoff = Duration::from_millis(250);
                    }
                    Err(e) => {
                        tracing::warn!(session = %cluihud_session_id, error = %e, "opencode SSE error; backing off");
                    }
                }
                // Coarse backoff between attempts, cancellable.
                tokio::select! {
                    _ = &mut cancel_rx => return,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = (backoff * 2).min(max_backoff);
            }
        });

        Self {
            handle: Some(handle),
            cancel: Some(cancel_tx),
        }
    }

    /// Signal the task to stop and await its termination.
    pub async fn cancel(mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
        }
    }
}

impl Drop for SseClient {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

async fn connect_and_stream(
    url: &str,
    cluihud_session_id: &str,
    port: u16,
    sink: &EventSink,
    pending: &Arc<DashMap<String, PendingPermission>>,
) -> Result<()> {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .with_context(|| format!("connecting to {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?;

    let mut stream = resp.bytes_stream().eventsource();
    while let Some(event) = stream.next().await {
        let event = event.map_err(|e| anyhow!("SSE framing error: {e}"))?;
        if let Some(translated) = translate_event(&event.data, cluihud_session_id, port, pending)
            && sink.send(translated).is_err()
        {
            tracing::debug!("event sink closed; ending SSE stream");
            return Ok(());
        }
    }
    Ok(())
}

/// Map an OpenCode SSE payload (the `data:` line, JSON of shape
/// `{type, properties}`) onto a [`HookEvent`] the rest of cluihud understands.
///
/// Returns `None` for events we deliberately ignore (TUI, server lifecycle,
/// etc. — see `docs/agents/opencode-sse-schema.md`). Errors during JSON
/// parsing are swallowed (logged at debug) so a single malformed event
/// doesn't tear down the stream.
pub(crate) fn translate_event(
    data: &str,
    cluihud_session_id: &str,
    port: u16,
    pending: &Arc<DashMap<String, PendingPermission>>,
) -> Option<HookEvent> {
    let payload: serde_json::Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, "skipping unparseable SSE event");
            return None;
        }
    };
    let event_type = payload.get("type")?.as_str()?;
    let props = payload.get("properties")?;

    match event_type {
        "session.idle" => Some(HookEvent::Stop {
            session_id: cluihud_session_id.to_string(),
            stop_reason: Some("session.idle".to_string()),
            transcript_path: None,
        }),

        "permission.asked" => {
            let permission_id = props.get("id").and_then(|v| v.as_str())?;
            let opencode_session_id = props.get("sessionID").and_then(|v| v.as_str())?;
            let prompt_text = props
                .get("permission")
                .and_then(|v| v.as_str())
                .unwrap_or("permission requested")
                .to_string();
            let fifo_path = format!("opencode://{cluihud_session_id}/{permission_id}");

            pending.insert(
                cluihud_session_id.to_string(),
                PendingPermission {
                    permission_id: permission_id.to_string(),
                    opencode_session_id: opencode_session_id.to_string(),
                    port,
                },
            );

            // Reuse the AskUser variant; the frontend already renders this
            // through the AskUserModal flow. The `tool_input` carries the
            // human-readable prompt + a single "questions" entry mimicking
            // the CC AskUserQuestion shape so AskUserModal renders unchanged.
            let tool_input = serde_json::json!({
                "questions": [{
                    "question": prompt_text,
                    "header": "OpenCode permission",
                    "options": ["allow once", "allow always", "reject"],
                    "multi_select": false
                }]
            });
            Some(HookEvent::AskUser {
                session_id: cluihud_session_id.to_string(),
                tool_input,
                fifo_path,
            })
        }

        "permission.replied" => {
            // Confirmation that our reply was applied. Drop the pending entry
            // (defence in depth — the adapter's submit_ask_answer also
            // removes it on the success path).
            pending.remove(cluihud_session_id);
            None
        }

        "todo.updated" => {
            // Forward the first todo as a TaskCreated event so the existing
            // TaskPanel surfaces it; the rest are best-effort. The richer
            // wire format (full todo array) is something we can plumb later
            // when TaskStore is generalised.
            let todos = props.get("todos").and_then(|v| v.as_array())?;
            let first = todos.first()?;
            let subject = first.get("content").and_then(|v| v.as_str())?.to_string();
            Some(HookEvent::TaskCreated {
                session_id: cluihud_session_id.to_string(),
                task_id: None,
                task_subject: Some(subject),
                tool_input: serde_json::Value::Null,
            })
        }

        // session.status, message.updated, message.part.updated carry rich
        // chat content the OpenCodeChat panel will render once shipped. Until
        // that lands we drop them — they're not actionable for the foundation.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_pending() -> Arc<DashMap<String, PendingPermission>> {
        Arc::new(DashMap::new())
    }

    #[test]
    fn translate_session_idle_emits_stop() {
        let data = r#"{"type":"session.idle","properties":{"sessionID":"ses_abc"}}"#;
        let pending = fresh_pending();
        let ev = translate_event(data, "cluihud-1", 14096, &pending).unwrap();
        match ev {
            HookEvent::Stop { session_id, .. } => assert_eq!(session_id, "cluihud-1"),
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn translate_permission_asked_stashes_pending_and_emits_ask_user() {
        let data = r#"{"type":"permission.asked","properties":{"id":"per_xyz","sessionID":"ses_abc","permission":"run rm -rf","patterns":[],"metadata":{},"always":[]}}"#;
        let pending = fresh_pending();
        let ev = translate_event(data, "cluihud-1", 14096, &pending).unwrap();
        match ev {
            HookEvent::AskUser { fifo_path, .. } => {
                assert_eq!(fifo_path, "opencode://cluihud-1/per_xyz");
            }
            other => panic!("expected AskUser, got {other:?}"),
        }
        let stashed = pending.get("cluihud-1").unwrap().clone();
        assert_eq!(stashed.permission_id, "per_xyz");
        assert_eq!(stashed.opencode_session_id, "ses_abc");
        assert_eq!(stashed.port, 14096);
    }

    #[test]
    fn translate_permission_replied_removes_pending() {
        let pending = fresh_pending();
        pending.insert(
            "cluihud-1".to_string(),
            PendingPermission {
                permission_id: "per_xyz".into(),
                opencode_session_id: "ses_abc".into(),
                port: 14096,
            },
        );
        let data = r#"{"type":"permission.replied","properties":{"sessionID":"ses_abc","requestID":"per_xyz","reply":"once"}}"#;
        assert!(translate_event(data, "cluihud-1", 14096, &pending).is_none());
        assert!(pending.get("cluihud-1").is_none());
    }

    #[test]
    fn translate_todo_updated_emits_first_task() {
        let data = r#"{"type":"todo.updated","properties":{"sessionID":"ses_abc","todos":[{"content":"do the thing","status":"pending","priority":"high"}]}}"#;
        let pending = fresh_pending();
        let ev = translate_event(data, "cluihud-1", 14096, &pending).unwrap();
        match ev {
            HookEvent::TaskCreated { task_subject, .. } => {
                assert_eq!(task_subject.as_deref(), Some("do the thing"));
            }
            other => panic!("expected TaskCreated, got {other:?}"),
        }
    }

    #[test]
    fn translate_unknown_event_returns_none() {
        let data = r#"{"type":"server.connected","properties":{}}"#;
        let pending = fresh_pending();
        assert!(translate_event(data, "cluihud-1", 14096, &pending).is_none());
    }

    #[test]
    fn translate_malformed_json_returns_none() {
        let pending = fresh_pending();
        assert!(translate_event("not json", "cluihud-1", 14096, &pending).is_none());
    }
}
