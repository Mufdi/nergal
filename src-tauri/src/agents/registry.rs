//! Adapter registry. Holds `Arc<dyn AgentAdapter>` instances keyed by
//! [`AgentId`], exposes `scan()` for parallel detection and a stable
//! [`AgentRegistry::priority_list`] for default-agent resolution.

use std::collections::HashMap;
use std::sync::Arc;

use super::{AdapterError, AgentAdapter, AgentId, DetectionResult};

/// In-process registry of adapter instances. Cheap to clone the `Arc`.
pub struct AgentRegistry {
    adapters: parking_lot::RwLock<HashMap<AgentId, Arc<dyn AgentAdapter>>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            adapters: parking_lot::RwLock::new(HashMap::new()),
        }
    }
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an adapter. Returns `Err(DuplicateAgentId)` if the id is
    /// already registered — registration is meant to happen once at startup.
    pub fn register(&self, adapter: Arc<dyn AgentAdapter>) -> Result<(), AdapterError> {
        let id = adapter.id();
        let mut w = self.adapters.write();
        if w.contains_key(&id) {
            return Err(AdapterError::DuplicateAgentId(id));
        }
        w.insert(id, adapter);
        Ok(())
    }

    /// Lookup by id. Returns `None` if unregistered (defensive — most call
    /// sites have a session row whose `agent_id` was validated at session
    /// creation, but a config drift could surface a stale id).
    pub fn get(&self, id: &AgentId) -> Option<Arc<dyn AgentAdapter>> {
        self.adapters.read().get(id).cloned()
    }

    /// All registered adapters, cloned out so the caller doesn't hold the
    /// read guard. Order is not guaranteed.
    pub fn list(&self) -> Vec<Arc<dyn AgentAdapter>> {
        self.adapters.read().values().cloned().collect()
    }

    /// Run `detect()` on every registered adapter, sequentially. Sequential is
    /// fine because the adapter set is small (≤4 in practice) and `detect()`
    /// is filesystem-only (no spawn). Parallelism would just add overhead.
    pub async fn scan(&self) -> Vec<(AgentId, DetectionResult)> {
        let adapters = self.list();
        let mut out = Vec::with_capacity(adapters.len());
        for a in adapters {
            let id = a.id();
            let det = a.detect().await;
            out.push((id, det));
        }
        out
    }

    /// Priority list for default-agent resolution when more than one is
    /// detected. Stable — codified here rather than derived from registration
    /// order so the user-visible default doesn't depend on init ordering.
    /// Lookup priority for the picker:
    /// `config.agent_overrides[project] > config.default_agent > priority_list`.
    pub fn priority_list() -> Vec<AgentId> {
        vec![
            AgentId::claude_code(),
            AgentId::codex(),
            AgentId::opencode(),
            AgentId::pi(),
        ]
    }
}

/// Register the Pi/Codex adapters that don't need a typed handle in
/// [`crate::agents::state::AgentRuntimeState`]. The CC and OpenCode adapters
/// are registered directly by `AgentRuntimeState::bootstrap` because the
/// runtime keeps typed `Arc<…>` handles for them (CC for FIFO registration
/// side-channels; OpenCode for chat-panel Tauri commands).
pub fn register_supplementary_adapters_excluding_opencode(
    reg: &AgentRegistry,
) -> Result<(), AdapterError> {
    reg.register(Arc::new(crate::agents::pi::PiAdapter::new()))?;
    reg.register(Arc::new(crate::agents::codex::CodexAdapter::new()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{
        AgentCapabilities, AgentCapability, EventSink, SpawnContext, SpawnSpec, TranscriptEvent,
        Transport,
    };
    use std::path::PathBuf;

    /// Minimal stub adapter for registry tests. Lives only in `cfg(test)`.
    struct StubAdapter {
        id: AgentId,
        caps: AgentCapabilities,
    }

    impl StubAdapter {
        fn new(id: AgentId) -> Self {
            Self {
                id,
                caps: AgentCapabilities {
                    flags: AgentCapability::empty(),
                    supported_models: vec![],
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl AgentAdapter for StubAdapter {
        fn id(&self) -> AgentId {
            self.id.clone()
        }
        fn display_name(&self) -> &str {
            "stub"
        }
        fn capabilities(&self) -> &AgentCapabilities {
            &self.caps
        }
        fn transport(&self) -> Transport {
            Transport::FileHooks {
                settings_path: PathBuf::new(),
                hook_event_names: vec![],
            }
        }
        fn requires_cluihud_setup(&self) -> bool {
            false
        }
        async fn detect(&self) -> DetectionResult {
            DetectionResult {
                installed: false,
                binary_path: None,
                config_path: None,
                version: None,
                trusted_for_project: None,
            }
        }
        fn spawn(&self, _ctx: &SpawnContext<'_>) -> Result<SpawnSpec, AdapterError> {
            Err(AdapterError::SessionLocked)
        }
        fn parse_transcript_line(&self, _line: &str) -> Option<TranscriptEvent> {
            None
        }
        async fn start_event_pump(
            &self,
            _session_id: &str,
            _sink: EventSink,
        ) -> Result<(), AdapterError> {
            Ok(())
        }
    }

    #[test]
    fn register_and_get_round_trips() {
        let reg = AgentRegistry::new();
        let id = AgentId::claude_code();
        reg.register(Arc::new(StubAdapter::new(id.clone())))
            .unwrap();
        assert!(reg.get(&id).is_some());
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let reg = AgentRegistry::new();
        let id = AgentId::claude_code();
        reg.register(Arc::new(StubAdapter::new(id.clone())))
            .unwrap();
        let err = reg
            .register(Arc::new(StubAdapter::new(id.clone())))
            .unwrap_err();
        match err {
            AdapterError::DuplicateAgentId(dup) => assert_eq!(dup, id),
            other => panic!("expected DuplicateAgentId, got {other:?}"),
        }
    }

    #[test]
    fn priority_list_has_all_known_ids_in_documented_order() {
        let p = AgentRegistry::priority_list();
        assert_eq!(
            p,
            vec![
                AgentId::claude_code(),
                AgentId::codex(),
                AgentId::opencode(),
                AgentId::pi(),
            ]
        );
    }

    #[tokio::test]
    async fn scan_runs_detect_on_each_registered_adapter() {
        let reg = AgentRegistry::new();
        reg.register(Arc::new(StubAdapter::new(AgentId::opencode())))
            .unwrap();
        reg.register(Arc::new(StubAdapter::new(AgentId::pi())))
            .unwrap();
        let results = reg.scan().await;
        assert_eq!(results.len(), 2);
        for (_, det) in results {
            assert!(!det.installed);
        }
    }
}
