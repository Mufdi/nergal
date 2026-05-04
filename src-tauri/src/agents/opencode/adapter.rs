//! OpenCode adapter — implements [`AgentAdapter`] over `opencode serve`'s
//! HTTP+SSE protocol.
//!
//! State held by this adapter:
//! - [`server_supervisor::ServerSupervisor`] — long-running `opencode serve`
//!   children, one per cluihud session.
//! - `sse_clients` — active SSE consumer tasks, keyed by cluihud session id.
//! - `pending_permissions` — outstanding `permission.asked` entries waiting
//!   for the user's reply. Populated by the SSE translator; drained by
//!   [`Self::submit_ask_answer`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use super::permission_client::{self, PendingPermission, Reply};
use super::server_supervisor::ServerSupervisor;
use super::sse_client::SseClient;
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, AuthScheme,
    DetectionResult, EventSink, SpawnContext, SpawnSpec, TranscriptEvent, Transport,
};

pub struct OpenCodeAdapter {
    capabilities: AgentCapabilities,
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    config_paths: Vec<PathBuf>,
    supervisor: Arc<ServerSupervisor>,
    sse_clients: Arc<DashMap<String, SseClient>>,
    pending_permissions: Arc<DashMap<String, PendingPermission>>,
}

impl Default for OpenCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            capabilities: AgentCapabilities {
                // Per docs/agents/opencode-sse-schema.md.
                flags: AgentCapability::ASK_USER_BLOCKING
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::RAW_COST_PER_MESSAGE
                    | AgentCapability::TASK_LIST
                    | AgentCapability::SESSION_RESUME,
                supported_models: vec![],
            },
            binary_path: parking_lot::RwLock::new(which::which("opencode").ok()),
            config_paths: vec![
                home.join(".config/opencode"),
                home.join(".local/share/opencode"),
            ],
            supervisor: Arc::new(ServerSupervisor::new()),
            sse_clients: Arc::new(DashMap::new()),
            pending_permissions: Arc::new(DashMap::new()),
        }
    }

    fn cached_binary(&self) -> Option<PathBuf> {
        self.binary_path.read().clone()
    }
}

#[async_trait]
impl AgentAdapter for OpenCodeAdapter {
    fn id(&self) -> AgentId {
        AgentId::opencode()
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn transport(&self) -> Transport {
        Transport::HttpSse {
            base_url_template: "http://127.0.0.1::port".into(),
            sse_path: "/event",
            permission_endpoint: "/session/{sessionID}/permissions/{permissionID}",
            // BYO credentials live in OpenCode's own config dir; cluihud
            // doesn't proxy auth headers (`opencode serve` runs locally).
            auth: Some(AuthScheme::None),
        }
    }

    fn requires_cluihud_setup(&self) -> bool {
        // No filesystem hooks to write — the SSE subscription is opened on
        // demand. `cluihud setup` does not need to touch OpenCode config.
        false
    }

    async fn detect(&self) -> DetectionResult {
        let binary_path = self
            .cached_binary()
            .or_else(|| which::which("opencode").ok());
        let config_path = self.config_paths.iter().find(|p| p.exists()).cloned();
        DetectionResult {
            installed: binary_path.is_some() || config_path.is_some(),
            binary_path,
            config_path,
            version: None,
            trusted_for_project: None,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let binary = self.cached_binary()?;
        let out = tokio::process::Command::new(&binary)
            .arg("--version")
            .output()
            .await
            .ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        // OpenCode sessions don't run a TUI in cluihud — they run as
        // `opencode serve` children supervised by start_event_pump. spawn()
        // emits a no-op SpawnSpec so the PTY layer doesn't try to launch
        // anything visible. The session id is still propagated as env so
        // any auxiliary tools the user runs in the workspace see it.
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.to_string());
        // `true` is a deliberate placeholder binary — it exits 0 immediately.
        // The OpenCode session has no terminal surface; the chat panel is
        // the entire UI.
        let binary = which::which("true").map_err(|e| {
            AdapterError::Transport(anyhow::anyhow!(
                "couldn't find `true` on PATH (required for placeholder spawn): {e}"
            ))
        })?;
        Ok(SpawnSpec {
            binary,
            args: vec![],
            env,
        })
    }

    fn parse_transcript_line(&self, _line: &str) -> Option<TranscriptEvent> {
        // OpenCode is event-driven (SSE); there is no transcript JSONL to
        // parse line-by-line. Cost / messages flow through the SSE event
        // pump instead. The trait method must remain — return None.
        None
    }

    async fn start_event_pump(
        &self,
        session_id: &str,
        sink: EventSink,
    ) -> Result<(), AdapterError> {
        let binary = self
            .cached_binary()
            .ok_or_else(|| AdapterError::Transport(anyhow::anyhow!("opencode binary not found")))?;

        let port = self
            .supervisor
            .start(session_id, &binary)
            .await
            .map_err(AdapterError::Transport)?;

        let base_url = format!("http://127.0.0.1:{port}");
        let client = SseClient::spawn(
            base_url,
            session_id.to_string(),
            port,
            sink,
            self.pending_permissions.clone(),
        );
        self.sse_clients.insert(session_id.to_string(), client);
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        if let Some((_, client)) = self.sse_clients.remove(session_id) {
            client.cancel().await;
        }
        self.supervisor
            .stop(session_id)
            .await
            .map_err(AdapterError::Transport)?;
        self.pending_permissions.remove(session_id);
        Ok(())
    }

    async fn submit_ask_answer(
        &self,
        session_id: &str,
        answers: serde_json::Value,
    ) -> Result<(), AdapterError> {
        let pending = self
            .pending_permissions
            .get(session_id)
            .map(|r| r.clone())
            .ok_or(AdapterError::SessionLocked)?;

        // Resolve the user's chosen option to a Reply variant. The frontend
        // renders OpenCode permission prompts through the AskUserModal which
        // returns a string; we accept the documented strings and a few
        // friendly aliases.
        let answer_str = answers
            .pointer("/0")
            .and_then(|v| v.as_str())
            .or_else(|| answers.as_str())
            .unwrap_or("once");

        let reply = match answer_str.to_lowercase().as_str() {
            "always" | "allow always" | "approve_always" => Reply::Always,
            "reject" | "deny" | "no" => Reply::Reject,
            // Default to a single approval — safer than always.
            _ => Reply::Once,
        };

        permission_client::submit_response(&pending, reply)
            .await
            .map_err(AdapterError::Transport)?;
        self.pending_permissions.remove(session_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_match_documented_schema() {
        let a = OpenCodeAdapter::new();
        let caps = a.capabilities().flags;
        assert!(!caps.contains(AgentCapability::PLAN_REVIEW));
        assert!(caps.contains(AgentCapability::ASK_USER_BLOCKING));
        assert!(caps.contains(AgentCapability::TASK_LIST));
        assert!(caps.contains(AgentCapability::RAW_COST_PER_MESSAGE));
        assert!(!caps.contains(AgentCapability::ANNOTATIONS_INJECT));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = OpenCodeAdapter::new();
        assert_eq!(a.id().as_str(), "opencode");
        assert_eq!(a.display_name(), "OpenCode");
    }

    #[test]
    fn transport_is_http_sse_with_correct_endpoints() {
        let a = OpenCodeAdapter::new();
        match a.transport() {
            Transport::HttpSse {
                sse_path,
                permission_endpoint,
                ..
            } => {
                assert_eq!(sse_path, "/event");
                assert!(permission_endpoint.contains("permissions"));
            }
            other => panic!("expected HttpSse, got {other:?}"),
        }
    }

    #[test]
    fn requires_cluihud_setup_is_false_for_opencode() {
        let a = OpenCodeAdapter::new();
        assert!(!a.requires_cluihud_setup());
    }

    #[test]
    fn parse_transcript_line_always_returns_none() {
        let a = OpenCodeAdapter::new();
        let line = r#"{"type":"session.idle","properties":{"sessionID":"ses_x"}}"#;
        assert!(a.parse_transcript_line(line).is_none());
    }

    #[tokio::test]
    async fn submit_ask_answer_without_pending_returns_session_locked() {
        let a = OpenCodeAdapter::new();
        let err = a
            .submit_ask_answer("no-session", serde_json::json!("once"))
            .await
            .unwrap_err();
        assert!(matches!(err, AdapterError::SessionLocked));
    }
}
