//! REST client for OpenCode permission replies.
//!
//! When the agent emits `permission.asked`, the SSE handler stashes a
//! [`PendingPermission`] keyed by cluihud session id. When the user answers,
//! the adapter looks up the pending entry and POSTs the reply to
//! `POST /session/{sessionID}/permissions/{permissionID}` with body
//! `{ "reply": "once" | "always" | "reject" }`.

use anyhow::{Context, Result};

/// One outstanding permission request waiting for the user's reply.
///
/// `port` is filled in by the adapter at registration time so the SSE
/// translator can stay decoupled from the supervisor.
#[derive(Debug, Clone)]
pub struct PendingPermission {
    pub permission_id: String,
    pub opencode_session_id: String,
    pub port: u16,
}

/// Reply variants OpenCode accepts on the permission endpoint. Mirrors the
/// `Event.permission.replied` schema's `reply` enum.
#[derive(Debug, Clone, Copy)]
pub enum Reply {
    Once,
    Always,
    Reject,
}

impl Reply {
    fn as_wire(self) -> &'static str {
        match self {
            Reply::Once => "once",
            Reply::Always => "always",
            Reply::Reject => "reject",
        }
    }
}

/// POST the reply to OpenCode. Surfaces non-2xx as an error so the caller can
/// log + remove the pending entry.
pub async fn submit_response(pending: &PendingPermission, reply: Reply) -> Result<()> {
    let url = format!(
        "http://127.0.0.1:{}/session/{}/permissions/{}",
        pending.port, pending.opencode_session_id, pending.permission_id
    );
    let body = serde_json::json!({ "reply": reply.as_wire() });
    reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx from {url}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_wire_values_match_documented_enum() {
        assert_eq!(Reply::Once.as_wire(), "once");
        assert_eq!(Reply::Always.as_wire(), "always");
        assert_eq!(Reply::Reject.as_wire(), "reject");
    }
}
