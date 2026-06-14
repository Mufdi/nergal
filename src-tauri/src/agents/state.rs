//! Runtime state shared between the hook dispatcher, session lifecycle
//! commands, and (eventually) the frontend bridge: a registry of installed
//! adapters plus a cache of `cluihud_session_id → AgentId` mappings.
//!
//! The cache is the hot-path resolution mechanism. Hook events arrive with a
//! `cluihud_session_id` and the dispatcher needs to know which adapter the
//! session belongs to without round-tripping the DB on every event. The cache
//! is populated by [`SessionLifecycle::register_session`] **before** the PTY
//! spawn (Decision 9), so the SessionStart hook never races the entry.

use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::AgentId;
use super::EventSink;
use super::claude_code::ClaudeCodeAdapter;
use super::registry::AgentRegistry;
use crate::hooks::events::HookEvent;

/// `(background_tasks, session_crons)` captured from a Stop payload, stored as
/// raw JSON values (pass-through, tolerant of CC shape drift).
pub type SessionBackground = (Vec<serde_json::Value>, Vec<serde_json::Value>);

/// Shared agent runtime state. Cheap to clone the inner `Arc`s.
#[derive(Clone)]
pub struct AgentRuntimeState {
    pub registry: Arc<AgentRegistry>,
    /// `cluihud_session_id` (the UUID cluihud assigns to a session) → adapter id.
    pub agent_id_cache: Arc<DashMap<String, AgentId>>,
    /// Typed handle to the CC adapter. The hook dispatcher and a few CC-
    /// specific commands need to call CC-only methods (registering pending
    /// FIFO paths from PlanReview / AskUser hook events). Going through
    /// `Arc<dyn AgentAdapter>` would require downcasting; keeping a typed
    /// `Arc<ClaudeCodeAdapter>` here is the ergonomic alternative for the
    /// foundation. Other adapters keep their typed handles in their own
    /// state when they need analogous side-channels.
    pub claude_code: Arc<ClaudeCodeAdapter>,
    /// Sender that adapter `start_event_pump` calls feed into when emitting
    /// translated [`HookEvent`]s. The runtime spawns a consumer task at
    /// startup (lib.rs) that drains the receiver and routes events through
    /// the existing dispatcher logic so the same Tauri events fire as for
    /// CC sessions.
    pub event_sink: EventSink,
    /// One-shot slot holding the receiver until the runtime takes it. Wrapped
    /// in a sync mutex (rare access, no need for tokio::Mutex).
    event_receiver: Arc<Mutex<Option<UnboundedReceiver<HookEvent>>>>,
    /// `claude_code_session_id → cluihud_session_id` side map for MCP identity
    /// resolution (a shim may know only CC's own id). Torn down alongside
    /// [`Self::forget_session`]. Empty until a CC id is learned; the primary
    /// path is the inherited `CLUIHUD_SESSION_ID` env hint validated directly
    /// against `agent_id_cache`.
    pub cc_session_map: Arc<DashMap<String, String>>,
    /// `cluihud_session_id → (background_tasks, session_crons)` captured from
    /// CC `Stop`/`SubagentStop` payloads (v2.1.150). Stored as raw JSON values
    /// (pass-through, tolerant of CC shape drift) and surfaced in the MCP
    /// session descriptor. Cleared on [`Self::forget_session`].
    session_background: Arc<DashMap<String, SessionBackground>>,
}

impl AgentRuntimeState {
    /// Build a fresh runtime state with the default set of adapters
    /// registered. The CC adapter ships in the foundation; OpenCode, Pi and
    /// Codex append themselves in their respective changes.
    pub fn bootstrap() -> Result<Self, super::AdapterError> {
        let registry = Arc::new(AgentRegistry::new());
        let claude_code = Arc::new(ClaudeCodeAdapter::new());
        registry.register(claude_code.clone())?;
        super::registry::register_supplementary_adapters(&registry)?;
        let (tx, rx) = unbounded_channel();
        Ok(Self {
            registry,
            agent_id_cache: Arc::new(DashMap::new()),
            claude_code,
            event_sink: tx,
            event_receiver: Arc::new(Mutex::new(Some(rx))),
            cc_session_map: Arc::new(DashMap::new()),
            session_background: Arc::new(DashMap::new()),
        })
    }

    /// Take the receiver. Called once during app setup; subsequent calls
    /// return `None`. The consumer task owns the receiver from then on.
    pub fn take_event_receiver(&self) -> Option<UnboundedReceiver<HookEvent>> {
        self.event_receiver.lock().take()
    }

    /// Record the agent that owns a session. Call this **before** the PTY
    /// spawn so the SessionStart hook always finds the entry.
    pub fn register_session(&self, cluihud_session_id: &str, agent_id: AgentId) {
        self.agent_id_cache
            .insert(cluihud_session_id.to_string(), agent_id);
    }

    /// Drop the cache entry for a session. Idempotent. Also tears down any
    /// `claude_code_session_id → cluihud_session_id` side-map entries pointing
    /// at this session so MCP identity can't resolve a dead session.
    pub fn forget_session(&self, cluihud_session_id: &str) {
        self.agent_id_cache.remove(cluihud_session_id);
        self.cc_session_map
            .retain(|_cc, csid| csid != cluihud_session_id);
        self.session_background.remove(cluihud_session_id);
    }

    /// Snapshot of the live session id set (the directory's liveness source:
    /// sessions with an entry in the agent cache). Owned, lock-free per entry.
    pub fn live_session_ids(&self) -> std::collections::HashSet<String> {
        self.agent_id_cache
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Record background tasks + crons captured from a `Stop`/`SubagentStop`
    /// payload for a session (overwrites the previous snapshot).
    pub fn set_session_background(
        &self,
        cluihud_session_id: &str,
        background_tasks: Vec<serde_json::Value>,
        session_crons: Vec<serde_json::Value>,
    ) {
        self.session_background.insert(
            cluihud_session_id.to_string(),
            (background_tasks, session_crons),
        );
    }

    /// `(background_tasks, session_crons)` for a session; empty when none seen.
    pub fn session_background(&self, cluihud_session_id: &str) -> SessionBackground {
        self.session_background
            .get(cluihud_session_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Record a `claude_code_session_id → cluihud_session_id` mapping for MCP
    /// identity resolution. No-op overwrite if already present.
    pub fn map_cc_session(&self, cc_session_id: &str, cluihud_session_id: &str) {
        self.cc_session_map
            .insert(cc_session_id.to_string(), cluihud_session_id.to_string());
    }

    /// Resolve an MCP-reported env hint to a live `cluihud_session_id`. Tries
    /// the hint as a cluihud id (the primary, inherited `CLUIHUD_SESSION_ID`
    /// path), then the CC side map. Returns `None` for an unknown id
    /// (unidentified caller).
    pub fn resolve_session_hint(&self, hint: &str) -> Option<String> {
        if self.agent_id_cache.contains_key(hint) {
            return Some(hint.to_string());
        }
        self.cc_session_map.get(hint).map(|r| r.clone())
    }

    /// Resolve a session to its agent id. Cache-only — the DB-fallback path
    /// belongs to the dispatcher (which has access to `SharedDb`); putting it
    /// here would muddy the separation between agent state and DB state.
    pub fn resolve(&self, cluihud_session_id: &str) -> Option<AgentId> {
        self.agent_id_cache
            .get(cluihud_session_id)
            .map(|r| r.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_registers_claude_code_adapter() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        assert!(state.registry.get(&AgentId::claude_code()).is_some());
    }

    #[test]
    fn register_then_resolve_round_trips() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.register_session("session-1", AgentId::claude_code());
        assert_eq!(state.resolve("session-1"), Some(AgentId::claude_code()));
    }

    #[test]
    fn forget_removes_cache_entry() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.register_session("s", AgentId::claude_code());
        state.forget_session("s");
        assert!(state.resolve("s").is_none());
    }

    #[test]
    fn unknown_session_resolves_to_none() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        assert!(state.resolve("never-registered").is_none());
    }

    #[test]
    fn hint_resolves_registered_cluihud_id() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.register_session("clui-1", AgentId::claude_code());
        assert_eq!(
            state.resolve_session_hint("clui-1").as_deref(),
            Some("clui-1")
        );
    }

    #[test]
    fn unknown_hint_is_unidentified() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        assert!(state.resolve_session_hint("nope").is_none());
    }

    #[test]
    fn cc_side_map_resolves_to_cluihud_id() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.register_session("clui-1", AgentId::claude_code());
        state.map_cc_session("cc-abc", "clui-1");
        assert_eq!(
            state.resolve_session_hint("cc-abc").as_deref(),
            Some("clui-1")
        );
    }

    #[test]
    fn forget_tears_down_cc_side_map() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.register_session("clui-1", AgentId::claude_code());
        state.map_cc_session("cc-abc", "clui-1");
        state.forget_session("clui-1");
        assert!(state.resolve_session_hint("cc-abc").is_none());
    }
}
