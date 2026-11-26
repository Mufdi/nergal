//! Runtime state shared between the hook dispatcher, session lifecycle
//! commands, and (eventually) the frontend bridge: a registry of installed
//! adapters plus a cache of `nergal_session_id → AgentId` mappings and the set
//! of sessions with a live PTY.
//!
//! The cache is the hot-path resolution mechanism. Hook events arrive with a
//! `nergal_session_id` and the dispatcher needs to know which adapter the
//! session belongs to without round-tripping the DB on every event. The cache
//! is populated by [`SessionLifecycle::register_session`] **before** the PTY
//! spawn (Decision 9), so the SessionStart hook never races the entry. It says
//! nothing about liveness: a late hook can register a session whose PTY is
//! gone. Liveness lives in `live_ptys`, owned by the PTY lifecycle.

use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use parking_lot::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::AgentId;
use super::EventSink;
use super::claude_code::ClaudeCodeAdapter;
use super::registry::AgentRegistry;
use crate::hooks::events::HookEvent;
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};

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

/// Root status of a session (OSC 7501) with the PTY that reported it, so the EOF of an
/// older PTY cannot remove the status of the newer one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedStatus {
    pub pty_id: String,
    pub status: ProgramStatus,
    pub set_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub previous: Option<ProgramStatus>,
    pub current: Option<ProgramStatus>,
}

pub fn combined_mode(
    hook: Option<&SessionActivity>,
    root: Option<&OwnedStatus>,
) -> (String, Option<String>, Option<u64>) {
    let hook_attention = hook.is_some_and(|h| h.mode == "needs_attention");
    let hook_waiting = hook.and_then(|h| h.waiting_for.clone());
    let last = match (hook.map(|h| h.last_activity), root.map(|r| r.set_at)) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    let Some(root) = root else {
        return match hook {
            Some(h) => (h.mode.clone(), hook_waiting, last),
            None => (String::new(), None, None),
        };
    };
    let status = &root.status;
    if status.state == RootState::Blocked {
        let waiting = match status.msg.as_deref().filter(|m| !m.is_empty()) {
            Some(msg) => msg.chars().take(200).collect(),
            None => match status.kind.as_deref() {
                Some("permission") => "permission".to_string(),
                Some("question") => "question".to_string(),
                Some("auth") => "login".to_string(),
                _ => "dialog".to_string(),
            },
        };
        return ("needs_attention".to_string(), Some(waiting), last);
    }
    if hook_attention {
        return ("needs_attention".to_string(), hook_waiting, last);
    }
    let mode = if status.state == RootState::Working {
        "running"
    } else {
        "idle"
    };
    (mode.to_string(), None, last)
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
    /// `nergal_session_id → pty id` of the PTY currently backing the session.
    /// Written only by the PTY lifecycle; the pty id lets a stale teardown
    /// (older PTY) avoid clearing a newer PTY's entry.
    live_ptys: Arc<DashMap<String, String>>,
    /// `nergal_session_id → root status` reported over OSC 7501. Kept apart from
    /// `session_activity`: writing the status there would reset the `just_idled`
    /// window and invent a hook mode.
    program_status: Arc<DashMap<String, OwnedStatus>>,
    /// `nergal_session_id → pty id` that has answered an OSC 7501 query, i.e. whose Claude Code
    /// has the protocol on. The agent PTY is an interactive shell, so reports printed by plain
    /// data (`cat`, `curl`) before or after Claude Code must not become the root status.
    program_status_armed: Arc<DashMap<String, String>>,
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
            live_ptys: Arc::new(DashMap::new()),
            program_status: Arc::new(DashMap::new()),
            program_status_armed: Arc::new(DashMap::new()),
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
        self.live_ptys.remove(nergal_session_id);
        self.program_status.remove(nergal_session_id);
        self.program_status_armed.remove(nergal_session_id);
    }

    /// Overwrites any prior pty id: the newest reservation owns the entry.
    pub fn mark_live(&self, session_id: &str, pty_id: &str) {
        self.live_ptys
            .insert(session_id.to_string(), pty_id.to_string());
    }

    pub fn live_pty(&self, session_id: &str) -> Option<String> {
        self.live_ptys.get(session_id).map(|p| p.clone())
    }

    pub fn mark_dead(&self, session_id: &str) {
        self.live_ptys.remove(session_id);
    }

    /// Clear liveness only if `pty_id` is still the session's PTY, so a late
    /// teardown of an old PTY cannot kill a newer one's entry.
    pub fn mark_dead_if(&self, session_id: &str, pty_id: &str) {
        self.live_ptys
            .remove_if(session_id, |_, current| current == pty_id);
    }

    pub fn apply_program_status(
        &self,
        session_id: &str,
        pty_id: &str,
        action: ReportAction,
        now: u64,
    ) -> Option<StatusChange> {
        let live = self.live_ptys.get(session_id).is_some_and(|p| *p == pty_id);
        if !live {
            return None;
        }
        match action {
            ReportAction::Ignore => None,
            ReportAction::Clear => {
                let (_, removed) = self.program_status.remove(session_id)?;
                Some(StatusChange {
                    previous: Some(removed.status),
                    current: None,
                })
            }
            ReportAction::Set(status) => match self.program_status.entry(session_id.to_string()) {
                Entry::Occupied(mut occupied) => {
                    let entry = occupied.get_mut();
                    entry.pty_id = pty_id.to_string();
                    if entry.status == status {
                        return None;
                    }
                    let previous = std::mem::replace(&mut entry.status, status.clone());
                    entry.set_at = now;
                    Some(StatusChange {
                        previous: Some(previous),
                        current: Some(status),
                    })
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(OwnedStatus {
                        pty_id: pty_id.to_string(),
                        status: status.clone(),
                        set_at: now,
                    });
                    Some(StatusChange {
                        previous: None,
                        current: Some(status),
                    })
                }
            },
        }
    }

    /// A newer PTY's arming replaces the older one's, so the old shell stops being believed.
    pub fn arm_program_status(&self, session_id: &str, pty_id: &str) {
        let live = self.live_ptys.get(session_id).is_some_and(|p| *p == pty_id);
        if !live {
            return;
        }
        self.program_status_armed
            .insert(session_id.to_string(), pty_id.to_string());
    }

    pub fn disarm_program_status(&self, session_id: &str) {
        self.program_status_armed.remove(session_id);
    }

    pub fn program_status_armed(&self, session_id: &str, pty_id: &str) -> bool {
        self.program_status_armed
            .get(session_id)
            .is_some_and(|armed| *armed == pty_id)
    }

    pub fn clear_program_status_if(&self, session_id: &str, pty_id: &str) -> Option<StatusChange> {
        self.program_status_armed
            .remove_if(session_id, |_, armed| armed == pty_id);
        let (_, removed) = self
            .program_status
            .remove_if(session_id, |_, owned| owned.pty_id == pty_id)?;
        Some(StatusChange {
            previous: Some(removed.status),
            current: None,
        })
    }

    pub fn program_status(&self, session_id: &str) -> Option<OwnedStatus> {
        self.program_status.get(session_id).map(|e| e.clone())
    }

    /// Snapshot of the live session id set: sessions with a live PTY (`live_ptys`),
    /// independent of `agent_id_cache`. Owned, lock-free per entry.
    pub fn live_session_ids(&self) -> std::collections::HashSet<String> {
        self.live_ptys.iter().map(|e| e.key().clone()).collect()
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

    /// Resolve an MCP-reported env hint to a session with a live PTY (the
    /// `live_ptys` set, not the adapter cache). Tries the hint as a nergal id
    /// (the inherited `NERGAL_SESSION_ID` path), then the CC side map. Returns
    /// `None` for an unknown or not-live id, so a killed session that still has
    /// an adapter cache entry is unidentified.
    pub fn resolve_session_hint(&self, hint: &str) -> Option<String> {
        if self.live_ptys.contains_key(hint) {
            return Some(hint.to_string());
        }
        self.cc_session_map
            .get(hint)
            .map(|r| r.clone())
            .filter(|id| self.live_ptys.contains_key(id))
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
#[path = "liveness_tests.rs"]
mod liveness_tests;

#[cfg(test)]
#[path = "identity_liveness_tests.rs"]
mod identity_liveness_tests;

#[cfg(test)]
#[path = "program_status_state_tests.rs"]
mod program_status_state_tests;

#[cfg(test)]
#[path = "program_status_arming_tests.rs"]
mod program_status_arming_tests;

#[cfg(test)]
#[path = "program_status_rereview_tests.rs"]
mod program_status_rereview_tests;

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
        state.mark_live("clui-1", "pty-test");
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
        state.mark_live("clui-1", "pty-test");
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
        state.mark_live("clui-1", "pty-test");
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
