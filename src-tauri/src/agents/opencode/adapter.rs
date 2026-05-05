//! OpenCode adapter — runs the `opencode` TUI inside the cluihud terminal
//! and listens to its embedded Hono server's SSE stream for events.
//!
//! The TUI binary serves the same HTTP API as `opencode serve`. We pin its
//! port via `--port <X>` so the SSE consumer can connect to a known address.
//! Events translated from `/event` flow through the runtime sink into the
//! same dispatcher CC's socket events use, so the Modified Files / Activity
//! panels light up identically — without restoring the old chat-panel UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

use super::sse_client::{SessionIdMap, SseClient};
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, DetectionResult,
    EventSink, SpawnContext, SpawnSpec, TranscriptEvent, Transport,
};

/// Port range reserved for OpenCode TUI sessions. Stays well above the
/// reserved range and out of common dev-server territory (3000s/8000s).
const PORT_BASE: u16 = 41000;
const PORT_SPAN: u16 = 1000;

pub struct OpenCodeAdapter {
    capabilities: AgentCapabilities,
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    config_paths: Vec<PathBuf>,
    /// `cluihud_session_id` → bound port. Set in [`Self::spawn`] and read by
    /// [`Self::start_event_pump`] so the SSE consumer connects to the right
    /// address without re-deriving it.
    session_ports: Arc<DashMap<String, u16>>,
    sse_clients: Arc<DashMap<String, SseClient>>,
    /// `cluihud_session_id` → OpenCode-side session id, harvested from the
    /// `session.created` SSE event. Surfaced via [`Self::agent_internal_session_id`]
    /// so the runtime can persist it and resume via `--session <id>`, which
    /// is more reliable than `--continue` (the OpenCode CLI's "last session"
    /// flag is global and doesn't reliably scope by cwd across restarts).
    session_internal_ids: SessionIdMap,
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
                // Now that the SSE consumer is wired, re-enable the
                // capabilities the chat-panel era exposed: tool-call events,
                // raw cost per message, and ASK_USER_BLOCKING flow back in
                // a follow-up. SESSION_RESUME stays — the TUI accepts
                // --continue / --session <id>.
                flags: AgentCapability::SESSION_RESUME
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT,
                supported_models: vec![],
            },
            binary_path: parking_lot::RwLock::new(which::which("opencode").ok()),
            config_paths: vec![
                home.join(".config/opencode"),
                home.join(".local/share/opencode"),
            ],
            session_ports: Arc::new(DashMap::new()),
            sse_clients: Arc::new(DashMap::new()),
            session_internal_ids: Arc::new(DashMap::new()),
        }
    }

    fn cached_binary(&self) -> Option<PathBuf> {
        self.binary_path.read().clone()
    }

    /// Pick a port deterministically from the session id so a session that
    /// reconnects after a frontend reload hits the same port. Multiple
    /// sessions hashing to the same port is unlikely (1/PORT_SPAN) and
    /// recovery is cheap (kill+respawn the TUI).
    fn port_for(session_id: &str) -> u16 {
        let mut hash: u32 = 5381;
        for b in session_id.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(*b as u32);
        }
        PORT_BASE + (hash % PORT_SPAN as u32) as u16
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
            auth: Some(crate::agents::AuthScheme::None),
        }
    }

    fn requires_cluihud_setup(&self) -> bool {
        false
    }

    async fn detect(&self) -> DetectionResult {
        let binary_path = self
            .cached_binary()
            .or_else(|| which::which("opencode").ok());
        let config_path = self.config_paths.iter().find(|p| p.exists()).cloned();
        DetectionResult {
            installed: binary_path.is_some(),
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
        let binary = self.cached_binary().ok_or_else(|| {
            AdapterError::Transport(anyhow::anyhow!("opencode binary not found on PATH"))
        })?;

        // Pin the TUI's embedded server to a known port so the SSE consumer
        // can attach. Stash for `start_event_pump`.
        let port = Self::port_for(ctx.session_id);
        self.session_ports.insert(ctx.session_id.to_string(), port);

        let mut args: Vec<String> = vec![
            "--port".into(),
            port.to_string(),
            "--hostname".into(),
            "127.0.0.1".into(),
        ];

        // Resume sentinels shared with the rest of the adapters:
        //   - "continue"    → `--continue`        (latest session)
        //   - "<any other>" → `--session <id>`    (specific session)
        //   - "resume_pick" → not supported (no picker UI); falls back to
        //                    `--continue`.
        match ctx.resume_from {
            None => {}
            Some("continue") | Some("resume_pick") => args.push("--continue".into()),
            Some(id) => {
                args.push("--session".into());
                args.push(id.to_string());
            }
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.to_string());
        Ok(SpawnSpec { binary, args, env })
    }

    fn parse_transcript_line(&self, _line: &str) -> Option<TranscriptEvent> {
        None
    }

    async fn start_event_pump(
        &self,
        session_id: &str,
        sink: EventSink,
    ) -> Result<(), AdapterError> {
        // Idempotent: a second call while a client is already running would
        // open a second `/event` subscription on the same port. Today the
        // duplication is mostly invisible (live-only stream, no catch-up)
        // but a long-lived second client still emits future events twice
        // until the first is replaced via DashMap insert. Mirrors the Pi
        // adapter's guard.
        if self.sse_clients.contains_key(session_id) {
            return Ok(());
        }

        // Read the port the spawn step picked — defensive lookup with a
        // fallback to the deterministic hash so a hot-reload of the
        // frontend (which may skip spawn) still finds the right address.
        let port = self
            .session_ports
            .get(session_id)
            .map(|r| *r)
            .unwrap_or_else(|| Self::port_for(session_id));

        let base_url = format!("http://127.0.0.1:{port}");
        let client = SseClient::spawn(
            base_url,
            session_id.to_string(),
            sink,
            self.session_internal_ids.clone(),
        );
        self.sse_clients.insert(session_id.to_string(), client);
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        if let Some((_, client)) = self.sse_clients.remove(session_id) {
            client.cancel().await;
        }
        self.session_ports.remove(session_id);
        // session_internal_ids stays — the runtime persists the id onto the
        // session row before teardown so resume continues to work.
        Ok(())
    }

    fn agent_internal_session_id(&self, cluihud_session_id: &str) -> Option<String> {
        self.session_internal_ids
            .get(cluihud_session_id)
            .map(|r| r.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_match_terminal_plus_sse_flow() {
        let a = OpenCodeAdapter::new();
        let caps = a.capabilities().flags;
        assert!(caps.contains(AgentCapability::SESSION_RESUME));
        assert!(caps.contains(AgentCapability::TOOL_CALL_EVENTS));
        // No native picker on the OpenCode CLI.
        assert!(!caps.contains(AgentCapability::SESSION_PICKER));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = OpenCodeAdapter::new();
        assert_eq!(a.id().as_str(), "opencode");
        assert_eq!(a.display_name(), "OpenCode");
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

    #[test]
    fn port_for_is_deterministic_and_in_range() {
        let p1 = OpenCodeAdapter::port_for("ses-abc");
        let p2 = OpenCodeAdapter::port_for("ses-abc");
        assert_eq!(p1, p2);
        assert!((PORT_BASE..PORT_BASE + PORT_SPAN).contains(&p1));
    }

    #[test]
    fn spawn_emits_port_and_hostname_flags() {
        let a = OpenCodeAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let ctx = SpawnContext {
            session_id: "ses-spawn",
            cwd,
            resume_from: None,
            initial_prompt: None,
        };
        if let Ok(spec) = a.spawn(&ctx) {
            assert!(spec.args.iter().any(|a| a == "--port"));
            assert!(spec.args.iter().any(|a| a == "--hostname"));
            assert!(spec.args.iter().any(|a| a == "127.0.0.1"));
        }
    }

    #[test]
    fn spawn_resume_modes_map_to_correct_opencode_flags() {
        let a = OpenCodeAdapter::new();
        let cwd = std::path::Path::new("/tmp");
        let mk = |resume: Option<&'static str>| SpawnContext {
            session_id: "s",
            cwd,
            resume_from: resume,
            initial_prompt: None,
        };
        if let Ok(spec) = a.spawn(&mk(Some("continue"))) {
            assert!(spec.args.iter().any(|a| a == "--continue"));
        }
        if let Ok(spec) = a.spawn(&mk(Some("resume_pick"))) {
            assert!(spec.args.iter().any(|a| a == "--continue"));
        }
        if let Ok(spec) = a.spawn(&mk(Some("abc-uuid"))) {
            assert!(spec.args.iter().any(|a| a == "--session"));
            assert!(spec.args.iter().any(|a| a == "abc-uuid"));
        }
    }
}
