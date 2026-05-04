//! Codex adapter — implements [`AgentAdapter`] reusing the FileHooks
//! transport. Hook events flow through the same Unix socket dispatcher
//! as CC; the adapter primarily contributes detection, cost extraction,
//! and the hooks.json setup helper.

use std::collections::HashMap;

use async_trait::async_trait;

use super::transcript::parse_transcript_line;
use crate::agents::{
    AdapterError, AgentAdapter, AgentCapabilities, AgentCapability, AgentId, DetectionResult,
    EventSink, SpawnContext, SpawnSpec, TranscriptEvent, Transport,
};

pub struct CodexAdapter {
    capabilities: AgentCapabilities,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self {
            capabilities: AgentCapabilities {
                // Codex has permission prompts via PermissionRequest hooks
                // (same shape as CC). It has no plan-mode equivalent and no
                // first-class task list; the rollout JSONL is structured but
                // not as rich as CC's transcript for annotations injection.
                flags: AgentCapability::ASK_USER_BLOCKING
                    | AgentCapability::TOOL_CALL_EVENTS
                    | AgentCapability::STRUCTURED_TRANSCRIPT
                    | AgentCapability::RAW_COST_PER_MESSAGE
                    | AgentCapability::SESSION_RESUME,
                supported_models: vec![],
            },
        }
    }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn id(&self) -> AgentId {
        AgentId::codex()
    }

    fn display_name(&self) -> &str {
        "Codex"
    }

    fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    fn transport(&self) -> Transport {
        let settings_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".codex/hooks.json");
        Transport::FileHooks {
            settings_path,
            hook_event_names: vec![
                "SessionStart",
                "SessionEnd",
                "PreToolUse",
                "PostToolUse",
                "PermissionRequest",
                "Stop",
                "UserPromptSubmit",
            ],
        }
    }

    fn requires_cluihud_setup(&self) -> bool {
        true
    }

    async fn detect(&self) -> DetectionResult {
        let home = dirs::home_dir().unwrap_or_default();
        let config_dir = home.join(".codex");
        let binary_path = which::which("codex").ok();
        let trusted_for_project = read_trust_for_cwd().await;
        DetectionResult {
            installed: config_dir.exists() || binary_path.is_some(),
            binary_path,
            config_path: if config_dir.exists() {
                Some(config_dir)
            } else {
                None
            },
            version: None,
            trusted_for_project,
        }
    }

    async fn refresh_version(&self) -> Option<String> {
        let out = tokio::process::Command::new("codex")
            .arg("--version")
            .output()
            .await
            .ok()?;
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if raw.is_empty() { None } else { Some(raw) }
    }

    fn spawn(&self, ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
        let binary = which::which("codex")
            .map_err(|e| AdapterError::Transport(anyhow::anyhow!("codex not on PATH: {e}")))?;
        let mut args: Vec<String> = Vec::new();
        if let Some(uuid) = ctx.resume_from {
            args.push("resume".into());
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
        _session_id: &str,
        _sink: EventSink,
    ) -> Result<(), AdapterError> {
        // Codex hook events arrive on the shared Unix socket (same as CC).
        // The rollout JSONL tail for cost extraction is wired through the
        // dispatcher when it gains adapter-aware routing — for now a no-op
        // keeps Codex's hook flow functional without spurious watchers.
        Ok(())
    }
}

/// Best-effort read of Codex's per-project trust state. Codex stores trust
/// in `~/.codex/config.toml` or a project-specific file; the exact format
/// shifts between versions. Returns `None` when the bit is unknown so the
/// UI shows a neutral state rather than asserting one way or the other.
async fn read_trust_for_cwd() -> Option<bool> {
    let home = dirs::home_dir()?;
    let trust_file = home.join(".codex").join("trust.json");
    if !trust_file.exists() {
        return None;
    }
    let content = tokio::fs::read_to_string(&trust_file).await.ok()?;
    let trust: serde_json::Value = serde_json::from_str(&content).ok()?;
    let cwd = std::env::current_dir().ok()?.display().to_string();
    trust.get(&cwd).and_then(|v| v.as_bool()).or(Some(false))
}

/// Re-export of [`super::setup::run_codex_setup`] for use by the
/// `cluihud setup` flow when the Codex adapter is selected.
pub use super::setup::run_codex_setup;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_match_codex_design() {
        let a = CodexAdapter::new();
        let caps = a.capabilities().flags;
        assert!(!caps.contains(AgentCapability::PLAN_REVIEW));
        assert!(caps.contains(AgentCapability::ASK_USER_BLOCKING));
        assert!(!caps.contains(AgentCapability::TASK_LIST));
        assert!(!caps.contains(AgentCapability::ANNOTATIONS_INJECT));
        assert!(caps.contains(AgentCapability::SESSION_RESUME));
    }

    #[test]
    fn id_and_display_name_are_stable() {
        let a = CodexAdapter::new();
        assert_eq!(a.id().as_str(), "codex");
        assert_eq!(a.display_name(), "Codex");
    }

    #[test]
    fn transport_targets_codex_hooks_json() {
        let a = CodexAdapter::new();
        match a.transport() {
            Transport::FileHooks {
                settings_path,
                hook_event_names,
            } => {
                assert!(settings_path.ends_with(".codex/hooks.json"));
                assert!(hook_event_names.contains(&"PreToolUse"));
                assert!(hook_event_names.contains(&"PermissionRequest"));
            }
            other => panic!("expected FileHooks, got {other:?}"),
        }
    }

    #[test]
    fn requires_cluihud_setup_is_true_for_codex() {
        let a = CodexAdapter::new();
        assert!(a.requires_cluihud_setup());
    }
}
