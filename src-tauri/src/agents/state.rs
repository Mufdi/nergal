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
use super::opencode::OpenCodeAdapter;
use super::registry::AgentRegistry;
use crate::hooks::events::HookEvent;

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
    /// Typed handle to the OpenCode adapter. Tauri commands for the chat
    /// panel (`opencode_send_prompt`, `opencode_list_messages`) route through
    /// it without going via the trait object — keeping the call sites concrete
    /// avoids dynamic-dispatch detours through downcasting.
    pub opencode: Arc<OpenCodeAdapter>,
    /// Sender that adapter `start_event_pump` calls feed into when emitting
    /// translated [`HookEvent`]s. The runtime spawns a consumer task at
    /// startup (lib.rs) that drains the receiver and routes events through
    /// the existing dispatcher logic so the same Tauri events fire as for
    /// CC sessions.
    pub event_sink: EventSink,
    /// One-shot slot holding the receiver until the runtime takes it. Wrapped
    /// in a sync mutex (rare access, no need for tokio::Mutex).
    event_receiver: Arc<Mutex<Option<UnboundedReceiver<HookEvent>>>>,
}

impl AgentRuntimeState {
    /// Build a fresh runtime state with the default set of adapters
    /// registered. The CC adapter ships in the foundation; OpenCode, Pi and
    /// Codex append themselves in their respective changes.
    pub fn bootstrap() -> Result<Self, super::AdapterError> {
        let registry = Arc::new(AgentRegistry::new());
        let claude_code = Arc::new(ClaudeCodeAdapter::new());
        registry.register(claude_code.clone())?;
        let opencode = Arc::new(OpenCodeAdapter::new());
        registry.register(opencode.clone())?;
        super::registry::register_supplementary_adapters_excluding_opencode(&registry)?;
        let (tx, rx) = unbounded_channel();
        Ok(Self {
            registry,
            agent_id_cache: Arc::new(DashMap::new()),
            claude_code,
            opencode,
            event_sink: tx,
            event_receiver: Arc::new(Mutex::new(Some(rx))),
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

    /// Drop the cache entry for a session. Idempotent.
    pub fn forget_session(&self, cluihud_session_id: &str) {
        self.agent_id_cache.remove(cluihud_session_id);
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
}
