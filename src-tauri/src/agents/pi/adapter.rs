//! Pi adapter — implements [`AgentAdapter`] over a JSONL tail.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;

use super::jsonl_tail::{JsonlTailHandle, LineParser, start_tail};
use super::session_resolver::{encode_cwd_to_pi_path, extract_pi_session_uuid, wait_for_jsonl};
use super::transcript::parse_transcript_line;
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, DetectionResult,
    EventSink, SpawnContext, SpawnSpec, TranscriptEvent, Transport,
};

pub struct PiAdapter {
    capabilities: AgentCapabilities,
    binary_path: parking_lot::RwLock<Option<PathBuf>>,
    state_dir: PathBuf,
    /// cwd captured at spawn() time. start_event_pump reads it to derive the
    /// encoded sessions directory. Stays small (one entry per active
    /// session); cleared on stop_event_pump.
    session_cwds: Arc<DashMap<String, PathBuf>>,
    tails: Arc<DashMap<String, JsonlTailHandle>>,
    /// Pi-internal session UUIDs harvested from each session's first JSONL
    /// line. Surfaced for resume — Pi accepts `--resume <uuid>` to continue.
    /// In a follow-up the runtime will persist this onto the session row's
    /// `agent_internal_session_id` column.
    session_uuids: Arc<DashMap<String, String>>,
}

impl Default for PiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PiAdapter {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            capabilities: AgentCapabilities {
                // Pi by design has no plan-mode, no permission prompt, no
                // task-list integration, no annotation injection.
                flags: AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::RAW_COST_PER_MESSAGE
                    | AgentCapability::SESSION_RESUME,
                supported_models: vec![],
            },
            binary_path: parking_lot::RwLock::new(which::which("pi").ok()),
            state_dir: home.join(".pi/agent"),
            session_cwds: Arc::new(DashMap::new()),
            tails: Arc::new(DashMap::new()),
            session_uuids: Arc::new(DashMap::new()),
        }
    }

    /// Pi-internal session UUID, if known. Set after the JSONL header is
    /// parsed in start_event_pump. The runtime can persist it as
    /// `agent_internal_session_id` for resume.
    pub fn agent_internal_session_id(&self, cluihud_session_id: &str) -> Option<String> {
        self.session_uuids
            .get(cluihud_session_id)
            .map(|r| r.clone())
    }
}

#[async_trait]
impl AgentAdapter for PiAdapter {
    fn id(&self) -> AgentId {
        AgentId::pi()
    }

    fn display_name(&self) -> &str {
        "Pi"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn transport(&self) -> Transport {
        Transport::JsonlTail {
            sessions_dir: self.state_dir.join("sessions"),
        }
    }

    fn requires_cluihud_setup(&self) -> bool {
        false
    }

    async fn detect(&self) -> DetectionResult {
        let binary_path = self
            .binary_path
            .read()
            .clone()
            .or_else(|| which::which("pi").ok());
        DetectionResult {
            installed: self.state_dir.exists() || binary_path.is_some(),
            binary_path,
            config_path: if self.state_dir.exists() {
                Some(self.state_dir.clone())
            } else {
                None
            },
            version: None,
            trusted_for_project: None,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let bin = self.binary_path.read().clone()?;
        let out = tokio::process::Command::new(&bin)
            .arg("--version")
            .output()
            .await
            .ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        let binary = self
            .binary_path
            .read()
            .clone()
            .ok_or_else(|| AdapterError::Transport(anyhow::anyhow!("pi binary not found")))?;

        // Capture cwd so start_event_pump can locate the JSONL.
        self.session_cwds
            .insert(ctx.session_id.to_string(), ctx.cwd.to_path_buf());

        let mut args: Vec<String> = Vec::new();
        if let Some(uuid) = ctx.resume_from {
            args.push("--resume".into());
            args.push(uuid.to_string());
        }
        let mut env = HashMap::new();
        env.insert("CLUIHUD_SESSION_ID".into(), ctx.session_id.to_string());
        Ok(SpawnSpec { binary, args, env })
    }

    fn parse_transcript_line(&self, line: &str) -> Option<TranscriptEvent> {
        parse_transcript_line(line)
    }

    async fn start_event_pump(
        &self,
        session_id: &str,
        sink: EventSink,
    ) -> Result<(), AdapterError> {
        let cwd = self
            .session_cwds
            .get(session_id)
            .map(|r| r.clone())
            .ok_or_else(|| {
                AdapterError::Transport(anyhow::anyhow!(
                    "no cwd recorded for pi session {session_id}; spawn() must run first"
                ))
            })?;

        let encoded = encode_cwd_to_pi_path(&cwd);
        let sessions_dir = self.state_dir.join("sessions").join(&encoded);
        let jsonl = wait_for_jsonl(&sessions_dir, Duration::from_secs(2))
            .await
            .map_err(AdapterError::Transport)?;

        // Stash Pi's session UUID so the runtime can persist it for resume.
        if let Ok(uuid) = extract_pi_session_uuid(&jsonl).await {
            self.session_uuids.insert(session_id.to_string(), uuid);
        }

        let parser: LineParser = Arc::new(parse_transcript_line);
        let handle = start_tail(jsonl, session_id.to_string(), sink, parser);
        self.tails.insert(session_id.to_string(), handle);
        Ok(())
    }

    async fn stop_event_pump(&self, session_id: &str) -> Result<(), AdapterError> {
        if let Some((_, handle)) = self.tails.remove(session_id) {
            handle.cancel().await;
        }
        self.session_cwds.remove(session_id);
        // session_uuids stays — the runtime persists it onto the session row
        // as part of session teardown so resume continues to work.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_match_pi_design_constraints() {
        let a = PiAdapter::new();
        let caps = a.capabilities().flags;
        assert!(!caps.contains(AgentCapability::PLAN_REVIEW));
        assert!(!caps.contains(AgentCapability::ASK_USER_BLOCKING));
        assert!(!caps.contains(AgentCapability::TASK_LIST));
        assert!(!caps.contains(AgentCapability::ANNOTATIONS_INJECT));
        assert!(caps.contains(AgentCapability::TOOL_CALL_EVENTS));
        assert!(caps.contains(AgentCapability::SESSION_RESUME));
        assert!(caps.contains(AgentCapability::RAW_COST_PER_MESSAGE));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = PiAdapter::new();
        assert_eq!(a.id().as_str(), "pi");
        assert_eq!(a.display_name(), "Pi");
    }

    #[test]
    fn transport_is_jsonl_tail_with_sessions_dir() {
        let a = PiAdapter::new();
        match a.transport() {
            Transport::JsonlTail { sessions_dir } => {
                assert!(sessions_dir.ends_with(".pi/agent/sessions"));
            }
            other => panic!("expected JsonlTail, got {other:?}"),
        }
    }

    #[test]
    fn requires_cluihud_setup_is_false_for_pi() {
        let a = PiAdapter::new();
        assert!(!a.requires_cluihud_setup());
    }
}
