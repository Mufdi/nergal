//! Runtime state shared between the hook dispatcher, session lifecycle
//! commands, and (eventually) the frontend bridge: a registry of installed
//! adapters plus a cache of `nergal_session_id → AgentId` mappings.
//!
//! The cache is the hot-path resolution mechanism. Hook events arrive with a
//! `nergal_session_id` and the dispatcher needs to know which adapter the
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

/// One recently touched file plus the tool that touched it (`Read`, `Edit`,
/// `Bash`, …), surfaced in the MCP descriptor so an observer sees *how* the
/// other agent is using each path, not just which paths.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TouchedFile {
    pub path: String,
    pub tool: String,
}

/// Live activity snapshot the hook dispatcher keeps per session and the MCP
/// descriptor reads for freshness. Mirrors the frontend `modeMapAtom` +
/// attention state; supersedes the frozen DB `Session.status`/`updated_at`.
#[derive(Clone, Debug)]
pub struct SessionActivity {
    /// `running` | `idle` | `needs_attention`.
    pub mode: String,
    pub last_activity: u64,
    /// What the session is blocked on while `needs_attention` (the permission
    /// prompt, the question, the plan review); `None` otherwise.
    pub waiting_for: Option<String>,
}

/// Shared agent runtime state. Cheap to clone the inner `Arc`s.
#[derive(Clone)]
pub struct AgentRuntimeState {
    pub registry: Arc<AgentRegistry>,
    /// `nergal_session_id` (the UUID nergal assigns to a session) → adapter id.
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
    /// `claude_code_session_id → nergal_session_id` side map for MCP identity
    /// resolution (a shim may know only CC's own id). Torn down alongside
    /// [`Self::forget_session`]. Empty until a CC id is learned; the primary
    /// path is the inherited `NERGAL_SESSION_ID` env hint validated directly
    /// against `agent_id_cache`.
    pub cc_session_map: Arc<DashMap<String, String>>,
    /// `nergal_session_id → (background_tasks, session_crons)` captured from
    /// CC `Stop`/`SubagentStop` payloads (v2.1.150). Stored as raw JSON values
    /// (pass-through, tolerant of CC shape drift) and surfaced in the MCP
    /// session descriptor. Cleared on [`Self::forget_session`].
    session_background: Arc<DashMap<String, SessionBackground>>,
    /// `nergal_session_id → (live_mode, last_activity_epoch_secs)`. The hook
    /// dispatcher records here on every agent-meaningful event; the MCP
    /// descriptor reads it for freshness. The DB `Session.status`/`updated_at`
    /// columns only move on lifecycle mutations (creation, rename, branch) and
    /// never during a turn, so they can't answer "is this session working right
    /// now". Mirrors the frontend `modeMapAtom`. Cleared on
    /// [`Self::forget_session`].
    session_activity: Arc<DashMap<String, SessionActivity>>,
    /// `nergal_session_id → recently touched file paths`, most-recent-first,
    /// deduped, bounded to [`Self::RECENT_FILES_CAP`]. Fed from tool events that
    /// name a file (reads included — cross-session awareness wants "what is the
    /// other agent looking at"). Surfaced as the descriptor's
    /// `recently_touched_files`. Cleared on [`Self::forget_session`].
    session_files: Arc<DashMap<String, Vec<TouchedFile>>>,
    /// `nergal_session_id → last assistant message` (CC's
    /// `last_assistant_message` from the Stop payload). Surfaced verbatim as the
    /// descriptor's `last_assistant_message` — NOT the `summary` field, which
    /// stays reserved for the phase-6 AI recap. Cleared on
    /// [`Self::forget_session`].
    session_last_message: Arc<DashMap<String, String>>,
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
            session_activity: Arc::new(DashMap::new()),
            session_files: Arc::new(DashMap::new()),
            session_last_message: Arc::new(DashMap::new()),
        })
    }

    /// Take the receiver. Called once during app setup; subsequent calls
    /// return `None`. The consumer task owns the receiver from then on.
    pub fn take_event_receiver(&self) -> Option<UnboundedReceiver<HookEvent>> {
        self.event_receiver.lock().take()
    }

    /// Record the agent that owns a session. Call this **before** the PTY
    /// spawn so the SessionStart hook always finds the entry.
    pub fn register_session(&self, nergal_session_id: &str, agent_id: AgentId) {
        self.agent_id_cache
            .insert(nergal_session_id.to_string(), agent_id);
    }

    /// Drop the cache entry for a session. Idempotent. Also tears down any
    /// `claude_code_session_id → nergal_session_id` side-map entries pointing
    /// at this session so MCP identity can't resolve a dead session.
    pub fn forget_session(&self, nergal_session_id: &str) {
        self.agent_id_cache.remove(nergal_session_id);
        self.cc_session_map
            .retain(|_cc, csid| csid != nergal_session_id);
        self.session_background.remove(nergal_session_id);
        self.session_activity.remove(nergal_session_id);
        self.session_files.remove(nergal_session_id);
        self.session_last_message.remove(nergal_session_id);
    }

    /// Snapshot of the live session id set (the directory's liveness source:
    /// sessions with an entry in the agent cache). Owned, lock-free per entry.
    pub fn live_session_ids(&self) -> std::collections::HashSet<String> {
        self.agent_id_cache
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Resolve an agent id to its verified headless print command (for the
    /// opt-in summarizer). `None` when the id is unregistered or the adapter
    /// has no verified non-interactive print mode.
    pub fn headless_print_command(&self, agent_id: &str) -> Option<super::HeadlessPrintCommand> {
        let id = AgentId::new(agent_id).ok()?;
        self.registry.get(&id)?.headless_print_command()
    }

    /// Record background tasks + crons captured from a `Stop`/`SubagentStop`
    /// payload for a session (overwrites the previous snapshot).
    pub fn set_session_background(
        &self,
        nergal_session_id: &str,
        background_tasks: Vec<serde_json::Value>,
        session_crons: Vec<serde_json::Value>,
    ) {
        self.session_background.insert(
            nergal_session_id.to_string(),
            (background_tasks, session_crons),
        );
    }

    /// `(background_tasks, session_crons)` for a session; empty when none seen.
    pub fn session_background(&self, nergal_session_id: &str) -> SessionBackground {
        self.session_background
            .get(nergal_session_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Record live activity for a session: set its mode (`running` | `idle` |
    /// `needs_attention`), the optional `waiting_for` reason, and stamp the
    /// moment. Called by the hook dispatcher for every agent-meaningful event
    /// so the MCP descriptor reflects work as it happens rather than the frozen
    /// DB row.
    pub fn record_activity(&self, nergal_session_id: &str, mode: &str, waiting_for: Option<&str>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.session_activity.insert(
            nergal_session_id.to_string(),
            SessionActivity {
                mode: mode.to_string(),
                last_activity: now,
                waiting_for: waiting_for.map(str::to_string),
            },
        );
    }

    /// Live activity snapshot for a session, or `None` when no event has been
    /// recorded since the daemon started (the descriptor then falls back to the
    /// persisted DB row).
    pub fn session_activity(&self, nergal_session_id: &str) -> Option<SessionActivity> {
        self.session_activity
            .get(nergal_session_id)
            .map(|r| r.clone())
    }

    /// Upper bound on a session's remembered touched files.
    const RECENT_FILES_CAP: usize = 15;

    /// Record a file a session just touched, with the tool that touched it
    /// (read or wrote). Moves an existing path to the front (most-recent-first,
    /// refreshing its tool) and caps the list.
    pub fn record_touched_file(&self, nergal_session_id: &str, path: &str, tool: &str) {
        let mut entry = self
            .session_files
            .entry(nergal_session_id.to_string())
            .or_default();
        entry.retain(|f| f.path != path);
        entry.insert(
            0,
            TouchedFile {
                path: path.to_string(),
                tool: tool.to_string(),
            },
        );
        entry.truncate(Self::RECENT_FILES_CAP);
    }

    /// Recently touched files for a session, most-recent-first; empty when none.
    pub fn session_files(&self, nergal_session_id: &str) -> Vec<TouchedFile> {
        self.session_files
            .get(nergal_session_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Record (or clear) a session's last assistant message. An empty/absent
    /// message clears any prior value so a stale line can't linger.
    pub fn set_session_last_message(&self, nergal_session_id: &str, message: Option<String>) {
        match message.filter(|s| !s.trim().is_empty()) {
            Some(s) => {
                self.session_last_message
                    .insert(nergal_session_id.to_string(), s);
            }
            None => {
                self.session_last_message.remove(nergal_session_id);
            }
        }
    }

    /// A session's last assistant message, or `None` when none captured.
    pub fn session_last_message(&self, nergal_session_id: &str) -> Option<String> {
        self.session_last_message
            .get(nergal_session_id)
            .map(|r| r.clone())
    }

    /// Record a `claude_code_session_id → nergal_session_id` mapping for MCP
    /// identity resolution. No-op overwrite if already present.
    pub fn map_cc_session(&self, cc_session_id: &str, nergal_session_id: &str) {
        self.cc_session_map
            .insert(cc_session_id.to_string(), nergal_session_id.to_string());
    }

    /// Resolve an MCP-reported env hint to a live `nergal_session_id`. Tries
    /// the hint as a nergal id (the primary, inherited `NERGAL_SESSION_ID`
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
    pub fn resolve(&self, nergal_session_id: &str) -> Option<AgentId> {
        self.agent_id_cache
            .get(nergal_session_id)
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
    fn hint_resolves_registered_nergal_id() {
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
    fn cc_side_map_resolves_to_nergal_id() {
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

    #[test]
    fn activity_is_absent_until_recorded_then_reflects_latest() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        assert!(state.session_activity("s").is_none());
        state.record_activity("s", "running", None);
        let a = state.session_activity("s").unwrap();
        assert_eq!(a.mode, "running");
        assert!(a.waiting_for.is_none());
        state.record_activity("s", "needs_attention", Some("permission prompt"));
        let a = state.session_activity("s").unwrap();
        assert_eq!(a.mode, "needs_attention");
        assert_eq!(a.waiting_for.as_deref(), Some("permission prompt"));
        // A later non-attention event clears the reason.
        state.record_activity("s", "idle", None);
        assert!(state.session_activity("s").unwrap().waiting_for.is_none());
    }

    #[test]
    fn forget_clears_activity() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.record_activity("s", "running", None);
        state.forget_session("s");
        assert!(state.session_activity("s").is_none());
    }

    #[test]
    fn touched_files_are_most_recent_first_and_deduped_with_tool() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.record_touched_file("s", "a.rs", "Read");
        state.record_touched_file("s", "b.rs", "Read");
        state.record_touched_file("s", "a.rs", "Edit"); // re-touch moves to front + refreshes tool
        let files = state.session_files("s");
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.tool.as_str()))
            .collect();
        assert_eq!(pairs, vec![("a.rs", "Edit"), ("b.rs", "Read")]);
    }

    #[test]
    fn touched_files_are_capped() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        for i in 0..30 {
            state.record_touched_file("s", &format!("f{i}.rs"), "Read");
        }
        let files = state.session_files("s");
        assert_eq!(files.len(), AgentRuntimeState::RECENT_FILES_CAP);
        assert_eq!(files[0].path, "f29.rs"); // newest first
    }

    #[test]
    fn forget_clears_touched_files() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        state.record_touched_file("s", "a.rs", "Read");
        state.forget_session("s");
        assert!(state.session_files("s").is_empty());
    }

    #[test]
    fn last_message_sets_and_empty_clears() {
        let state = AgentRuntimeState::bootstrap().unwrap();
        assert!(state.session_last_message("s").is_none());
        state.set_session_last_message("s", Some("did the thing".into()));
        assert_eq!(
            state.session_last_message("s").as_deref(),
            Some("did the thing")
        );
        // A blank/None message clears the stale value.
        state.set_session_last_message("s", Some("   ".into()));
        assert!(state.session_last_message("s").is_none());
    }
}
