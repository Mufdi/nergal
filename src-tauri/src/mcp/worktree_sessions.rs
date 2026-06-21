//! agent-spawned-worktrees: an agent REQUESTS a worktree session (it never
//! creates one directly), and a mandatory, structurally un-bypassable human gate
//! approves it. See `openspec/changes/agent-spawned-worktrees/`.
//!
//! Architecture:
//! - The MCP tools (`create_worktree_session` / `get_worktree_request_status` /
//!   `cancel_worktree_request`) run in the daemon's sync `dispatch`; they only
//!   touch the in-memory [`WorktreeGateState`] and return immediately — a tool
//!   call must never block on human latency.
//! - The human gate is a set of Tauri commands (`list_worktree_requests` /
//!   `approve_worktree_request` / `deny_worktree_request`). The SOLE approval
//!   entry point is `approve_worktree_request`, invokable only from the GUI —
//!   no MCP/permission-mode handler can reach it, so the gate is un-bypassable
//!   by construction (CC `--permission-mode`/bypass has no path to a Tauri cmd).
//! - Two structures under one lock (atomicity for the timeout-vs-approve race):
//!   a `pending` map and a terminal-status `ledger`. When a pending entry is
//!   removed the terminal state is written to the ledger in the same critical
//!   section, so a poll still answers after the queue purge.
//! - State is volatile: a daemon restart empties both maps → in-flight requests
//!   are abandoned (no push attempted, poll returns `not_found`). Persisting a
//!   pending request could let a stale approval create an orphan post-restart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{Value, json};
use tauri::AppHandle;

use crate::agents::state::AgentRuntimeState;
use crate::config::AgentWorktreesConfig;
use crate::mcp::DaemonContext;
use crate::mcp::delivery::{AppBridge, SessionDelivery};
use crate::models::LaunchOptions;

/// Lower bound on a request timeout (a too-short timeout would race the gate UI).
const MIN_REQUEST_TIMEOUT_SECS: u64 = 60;
/// Upper bound on a request timeout (24h) — a request older than this is stale.
const MAX_REQUEST_TIMEOUT_SECS: u64 = 86_400;
/// How long a terminal status lingers in the ledger after resolution, so an
/// agent that polls late still learns the outcome before it ages to `not_found`.
const LEDGER_RETENTION_SECS: u64 = 3600;
/// Sweeper cadence: timeout purge + ledger GC.
const SWEEP_TICK_SECS: u64 = 30;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What a request asks for: a brand-new worktree session, or reviving an
/// existing inactive one. Both flow through the SAME human gate.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestKind {
    /// Create a brand-new worktree session under `workspace_id`.
    Create,
    /// Revive an existing, currently-inactive session in its own worktree and
    /// (optionally) deliver `prompt` as a labeled relayed message to it.
    Resume { target_session_id: String },
}

/// A request awaiting the human gate. Serialized to the gate UI. Every
/// agent-chosen escalation input is broken out so the human sees it explicitly
/// and none can ride in unseen inside the generic `launch_options` blob
/// (security review): `agent`, `permission_preset`, the verbatim shell
/// `startup_command` (runs as a PTY prelude — arbitrary code), and
/// `allow_skip_in_cycle` (adds bypass to the Shift+Tab mode cycle). Resume
/// requests carry no escalation inputs (they reuse the target's own options).
#[derive(Debug, Clone, Serialize)]
pub struct PendingWorktreeRequest {
    pub id: String,
    pub kind: RequestKind,
    pub requesting_session: String,
    pub workspace_id: String,
    pub repo_path: PathBuf,
    pub branch_name: Option<String>,
    pub prompt: String,
    /// Agent CLI the requester asked for (agent-chosen escalation input).
    pub agent: Option<String>,
    /// Permission preset the requester asked for (agent-chosen escalation input).
    pub permission_preset: Option<String>,
    /// Verbatim shell prelude the requester asked for. Runs as `cd && <cmd> &&
    /// <agent>` at spawn — ARBITRARY CODE EXECUTION, so the gate must surface it
    /// prominently; a benign-looking prompt could hide a malicious prelude.
    pub startup_command: Option<String>,
    /// Whether the requester asked to add bypass to the in-session mode cycle.
    pub allow_skip_in_cycle: bool,
    pub launch_options: Option<LaunchOptions>,
    pub created_at: u64,
    pub timeout_secs: u64,
}

/// Terminal outcome of a request, stored in the ledger and returned by a poll.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WorktreeRequestState {
    Pending,
    Approved { session_id: String },
    Denied,
    TimedOut,
    Cancelled,
    Failed { reason: String },
    NotFound,
}

#[derive(Debug, Clone)]
struct LedgerEntry {
    state: WorktreeRequestState,
    resolved_at: u64,
}

#[derive(Default)]
struct GateInner {
    pending: HashMap<String, PendingWorktreeRequest>,
    ledger: HashMap<String, LedgerEntry>,
    /// Outcome notes that could not be pushed at resolution time because the
    /// requester was working/awaiting (pasting then would corrupt its turn).
    /// Drained on the requester's next working→idle `Stop` (mirrors the
    /// cross-session idle-transition drain), so an outcome is never stranded.
    undelivered: HashMap<String, Vec<String>>,
}

/// Shared in-memory queue + terminal ledger. Cheap to clone (inner `Arc`).
/// Held both by the MCP `DaemonContext` (for the tools) and as Tauri managed
/// state (for the gate commands) — the same underlying maps.
#[derive(Clone, Default)]
pub struct WorktreeGateState {
    inner: Arc<Mutex<GateInner>>,
}

impl WorktreeGateState {
    fn lock(&self) -> std::sync::MutexGuard<'_, GateInner> {
        // A poisoned lock means a prior holder panicked mid-mutation. The maps
        // are plain data; recovering the guard is safe and preferable to
        // propagating the panic across the whole gate.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn pending_count_for(&self, session: &str) -> usize {
        self.lock()
            .pending
            .values()
            .filter(|r| r.requesting_session == session)
            .count()
    }

    fn insert_pending(&self, req: PendingWorktreeRequest) {
        self.lock().pending.insert(req.id.clone(), req);
    }

    /// Atomically remove a pending entry. The single winner of an
    /// approve-vs-timeout-vs-cancel race gets `Some`; everyone else gets `None`
    /// and must abort — this is the orphan-prevention guard.
    fn take_pending(&self, id: &str) -> Option<PendingWorktreeRequest> {
        self.lock().pending.remove(id)
    }

    /// Write a terminal state to the ledger (call after `take_pending`).
    fn resolve(&self, id: &str, state: WorktreeRequestState) {
        self.lock().ledger.insert(
            id.to_string(),
            LedgerEntry {
                state,
                resolved_at: now_secs(),
            },
        );
    }

    fn status(&self, id: &str) -> WorktreeRequestState {
        let g = self.lock();
        if g.pending.contains_key(id) {
            return WorktreeRequestState::Pending;
        }
        g.ledger
            .get(id)
            .map(|e| e.state.clone())
            .unwrap_or(WorktreeRequestState::NotFound)
    }

    fn list_pending(&self) -> Vec<PendingWorktreeRequest> {
        let mut v: Vec<_> = self.lock().pending.values().cloned().collect();
        v.sort_by_key(|r| r.created_at);
        v
    }

    /// Atomically (one critical section) move a pending request to a terminal
    /// `cancelled` ledger entry IF it is pending AND the caller owns it. Returns
    /// the resulting status either way (so a non-owner / already-resolved poll
    /// gets the truth, not a spurious success).
    fn cancel(&self, id: &str, caller: &str) -> WorktreeRequestState {
        let mut g = self.lock();
        match g.pending.get(id) {
            Some(req) if req.requesting_session == caller => {
                g.pending.remove(id);
                g.ledger.insert(
                    id.to_string(),
                    LedgerEntry {
                        state: WorktreeRequestState::Cancelled,
                        resolved_at: now_secs(),
                    },
                );
                WorktreeRequestState::Cancelled
            }
            Some(_) => WorktreeRequestState::Pending, // not the owner — unchanged
            None => g
                .ledger
                .get(id)
                .map(|e| e.state.clone())
                .unwrap_or(WorktreeRequestState::NotFound),
        }
    }

    /// Atomically take every pending request past its deadline and write
    /// `timed_out` to the ledger for each — so a concurrent approve cannot race
    /// a timeout into an orphaned session. Returns the taken requests so the
    /// caller can notify the requesters (outside the lock).
    fn due_timeouts(&self, now: u64) -> Vec<PendingWorktreeRequest> {
        let mut g = self.lock();
        let due: Vec<String> = g
            .pending
            .iter()
            .filter(|(_, r)| now.saturating_sub(r.created_at) >= r.timeout_secs)
            .map(|(id, _)| id.clone())
            .collect();
        let mut taken = Vec::with_capacity(due.len());
        for id in due {
            if let Some(req) = g.pending.remove(&id) {
                g.ledger.insert(
                    id.clone(),
                    LedgerEntry {
                        state: WorktreeRequestState::TimedOut,
                        resolved_at: now,
                    },
                );
                taken.push(req);
            }
        }
        taken
    }

    /// Drop ledger entries past the retention TTL (they become `not_found`).
    fn gc(&self, now: u64) {
        self.lock()
            .ledger
            .retain(|_, e| now.saturating_sub(e.resolved_at) < LEDGER_RETENTION_SECS);
    }

    /// Queue an outcome note for a session that could not be woken now (it was
    /// working/awaiting). Drained on its next idle transition.
    fn enqueue_outcome(&self, session: &str, note: String) {
        self.lock()
            .undelivered
            .entry(session.to_string())
            .or_default()
            .push(note);
    }

    /// Remove and return a session's queued outcome notes (for the idle drain).
    fn take_outcomes(&self, session: &str) -> Vec<String> {
        self.lock().undelivered.remove(session).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// MCP tools (sync, called from `dispatch`; only touch the in-memory gate).
// ---------------------------------------------------------------------------

/// `create_worktree_session` — enqueue a request and return its id immediately.
/// Never creates anything; never blocks for the human decision.
#[allow(clippy::too_many_arguments)] // request parameters map 1:1 to the MCP tool surface.
pub fn create_worktree_session(
    ctx: &DaemonContext,
    cfg: &AgentWorktreesConfig,
    requesting_session: &str,
    workspace_id: &str,
    prompt: &str,
    branch_name: Option<&str>,
    agent: Option<&str>,
    launch_options: Option<LaunchOptions>,
) -> Value {
    if !cfg.enabled {
        return json!({
            "status": "disabled",
            "hint": "enable agent-spawned worktrees in cluihud Settings → MCP",
        });
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return json!({ "status": "invalid_request", "message": "prompt is required" });
    }
    // Resolve + validate the target workspace (must be a known git repo).
    let repo_path = match ctx.with_db(|db| db.workspace_repo_path(workspace_id)) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return json!({
                "status": "invalid_workspace",
                "message": "no active workspace with that id",
            });
        }
        Err(e) => return json!({ "status": "error", "message": e.to_string() }),
    };
    if !crate::worktree::is_git_repo(&repo_path) {
        return json!({
            "status": "invalid_workspace",
            "message": "workspace is not a git repository",
        });
    }
    // Reject an unknown agent at request time (don't surface a bogus escalation
    // input to the human or fail late at approve).
    if let Some(a) = agent
        && crate::agents::AgentId::new(a).is_err()
    {
        return json!({ "status": "invalid_request", "message": format!("unknown agent: {a}") });
    }
    if ctx.worktree_gate.pending_count_for(requesting_session)
        >= cfg.max_pending_per_session as usize
    {
        return json!({
            "status": "too_many_pending_requests",
            "max": cfg.max_pending_per_session,
        });
    }

    let permission_preset = launch_options
        .as_ref()
        .and_then(|lo| serde_json::to_value(lo.permission_preset).ok())
        .and_then(|v| v.as_str().map(str::to_string));
    let startup_command = launch_options
        .as_ref()
        .and_then(|lo| lo.startup_command.clone())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let allow_skip_in_cycle = launch_options
        .as_ref()
        .map(|lo| lo.allow_skip_in_cycle)
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let req = PendingWorktreeRequest {
        id: id.clone(),
        kind: RequestKind::Create,
        requesting_session: requesting_session.to_string(),
        workspace_id: workspace_id.to_string(),
        repo_path,
        branch_name: branch_name.map(str::to_string),
        prompt: prompt.to_string(),
        agent: agent.map(str::to_string),
        permission_preset,
        startup_command,
        allow_skip_in_cycle,
        launch_options,
        created_at: now_secs(),
        timeout_secs: cfg
            .request_timeout_secs
            .clamp(MIN_REQUEST_TIMEOUT_SECS, MAX_REQUEST_TIMEOUT_SECS),
    };
    ctx.worktree_gate.insert_pending(req);
    // Best-effort nudge to the gate UI to refetch the queue.
    ctx.delivery
        .emit("worktree:request", json!({ "request_id": id }));
    json!({ "status": "pending", "pending_request_id": id })
}

/// `request_session_resume` — request reviving an existing, currently-inactive
/// session (e.g. yesterday's session A holds context session B needs today),
/// behind the SAME human gate. Non-blocking. On approve the session is resumed
/// in its own worktree and the optional `message` is delivered to it as a
/// labeled, advisory relayed prompt. Resume carries NO escalation inputs — it
/// reuses the target's own launch options.
pub fn request_session_resume(
    ctx: &DaemonContext,
    cfg: &AgentWorktreesConfig,
    requesting_session: &str,
    session_id: &str,
    message: Option<&str>,
) -> Value {
    if !cfg.enabled {
        return json!({
            "status": "disabled",
            "hint": "enable agent-spawned worktrees in cluihud Settings → MCP",
        });
    }
    // The target must exist (a known prior session) and not be live already.
    let session = match ctx.with_db(|db| db.find_session(session_id)) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return json!({
                "status": "unknown_session",
                "message": "no session with that id (use list_sessions/search_sessions to find one)",
            });
        }
        Err(e) => return json!({ "status": "error", "message": e.to_string() }),
    };
    if ctx
        .agents
        .live_session_ids()
        .iter()
        .any(|id| id == session_id)
    {
        return json!({
            "status": "already_live",
            "hint": "the session is already live — message it directly with send_to_session",
        });
    }
    if ctx.worktree_gate.pending_count_for(requesting_session)
        >= cfg.max_pending_per_session as usize
    {
        return json!({
            "status": "too_many_pending_requests",
            "max": cfg.max_pending_per_session,
        });
    }
    let repo_path = ctx
        .with_db(|db| db.workspace_repo_path(&session.workspace_id))
        .ok()
        .flatten()
        .unwrap_or_default();

    let id = uuid::Uuid::new_v4().to_string();
    let req = PendingWorktreeRequest {
        id: id.clone(),
        kind: RequestKind::Resume {
            target_session_id: session_id.to_string(),
        },
        requesting_session: requesting_session.to_string(),
        workspace_id: session.workspace_id.clone(),
        repo_path,
        branch_name: session.worktree_branch.clone(),
        prompt: message.unwrap_or("").trim().to_string(),
        agent: Some(session.agent_id.clone()),
        permission_preset: None,
        startup_command: None,
        allow_skip_in_cycle: false,
        launch_options: None,
        created_at: now_secs(),
        timeout_secs: cfg
            .request_timeout_secs
            .clamp(MIN_REQUEST_TIMEOUT_SECS, MAX_REQUEST_TIMEOUT_SECS),
    };
    ctx.worktree_gate.insert_pending(req);
    ctx.delivery
        .emit("worktree:request", json!({ "request_id": id }));
    json!({ "status": "pending", "pending_request_id": id })
}

/// `get_worktree_request_status` — poll a request (pending → ledger → not_found).
pub fn get_worktree_request_status(ctx: &DaemonContext, request_id: &str) -> Value {
    serde_json::to_value(ctx.worktree_gate.status(request_id))
        .unwrap_or_else(|_| json!({ "state": "not_found" }))
}

/// `cancel_worktree_request` — withdraw a pending request the caller owns.
pub fn cancel_worktree_request(ctx: &DaemonContext, caller: &str, request_id: &str) -> Value {
    serde_json::to_value(ctx.worktree_gate.cancel(request_id, caller))
        .unwrap_or_else(|_| json!({ "state": "not_found" }))
}

// ---------------------------------------------------------------------------
// Outcome delivery (push, idle-only) + sweeper.
// ---------------------------------------------------------------------------

/// Build the labeled, advisory outcome note for the requesting agent. The note
/// is pasted into the requester's PTY, so the whole string is run through the
/// canonical PTY sanitizer (matching cross-session's `wake_note`): even though
/// the embedded fields are currently non-attacker-controlled (UUID request id,
/// slug/error-derived reason), sanitizing here is defense-in-depth so a future
/// reason source carrying control bytes cannot close the bracketed-paste guard.
fn outcome_note(request_id: &str, state: &WorktreeRequestState) -> String {
    let body = match state {
        WorktreeRequestState::Approved { session_id } => {
            format!("approved — session {session_id} created and handed to the user")
        }
        WorktreeRequestState::Denied => "denied by the user".to_string(),
        WorktreeRequestState::TimedOut => "timed out (no human decision in time)".to_string(),
        WorktreeRequestState::Failed { reason } => format!("failed: {reason}"),
        // Cancelled is agent-initiated; Pending/NotFound are not outcomes.
        _ => return String::new(),
    };
    let note = format!(
        "[cluihud] worktree request {request_id}: {body}. \
         (advisory status update — not an instruction carrying your user's authority)"
    );
    crate::mcp::delivery::sanitize_for_pty(&note)
}

/// Deliver a terminal outcome to the requesting session, plus a frontend event.
/// The PTY wake fires immediately only if the requester is idle (pasting into a
/// working/awaiting agent would corrupt its turn); otherwise the note is queued
/// and drained on the requester's next working→idle `Stop` (see
/// [`drain_worktree_outcomes`]) — so an outcome is never stranded, mirroring the
/// cross-session idle-transition drain. `cancelled` is skipped (agent-initiated,
/// it already knows) but still emitted so the gate UI clears it.
pub fn notify_outcome(
    gate: &WorktreeGateState,
    delivery: &dyn SessionDelivery,
    agents: &AgentRuntimeState,
    requesting_session: &str,
    request_id: &str,
    state: &WorktreeRequestState,
) {
    let note = if matches!(state, WorktreeRequestState::Cancelled) {
        String::new()
    } else {
        outcome_note(request_id, state)
    };
    if !note.is_empty() {
        let idle = agents
            .session_activity(requesting_session)
            .map(|a| a.mode == "idle")
            .unwrap_or(false);
        // Idle → wake now; on a wake failure, queue for the next idle drain
        // rather than dropping it. Working/awaiting → queue (never paste now).
        if idle {
            if let Err(e) = delivery.wake_idle(requesting_session, &note) {
                tracing::debug!(requesting_session, "worktree outcome wake failed: {e:#}");
                gate.enqueue_outcome(requesting_session, note);
            }
        } else {
            gate.enqueue_outcome(requesting_session, note);
        }
    }
    delivery.emit(
        "worktree:resolved",
        json!({ "request_id": request_id, "requesting_session": requesting_session }),
    );
}

/// Deliver any queued worktree outcomes to a now-idle session by waking its PTY.
/// Called from the hook server on a `Stop` (working→idle), the same liveness
/// point cross-session uses. Best-effort: a wake failure re-queues the notes so
/// the next idle flip retries (never stranded, never silently dropped).
pub fn drain_worktree_outcomes(
    gate: &WorktreeGateState,
    delivery: &dyn SessionDelivery,
    session_id: &str,
) {
    let notes = gate.take_outcomes(session_id);
    if notes.is_empty() {
        return;
    }
    let combined = notes.join("\n");
    if let Err(e) = delivery.wake_idle(session_id, &combined) {
        tracing::debug!(session_id, "worktree outcome drain wake failed: {e:#}");
        // Re-queue verbatim so a later idle flip retries.
        for note in notes {
            gate.enqueue_outcome(session_id, note);
        }
    }
}

/// Background sweeper: atomically purge timed-out requests (notifying their
/// requesters) and GC the ledger. Mirrors `messaging::run_deadline_sweeper`.
pub async fn run_worktree_request_sweeper(
    app: AppHandle,
    agents: AgentRuntimeState,
    gate: WorktreeGateState,
) {
    let bridge = AppBridge::new(app);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(SWEEP_TICK_SECS)).await;
        let now = now_secs();
        for req in gate.due_timeouts(now) {
            notify_outcome(
                &gate,
                &bridge,
                &agents,
                &req.requesting_session,
                &req.id,
                &WorktreeRequestState::TimedOut,
            );
        }
        gate.gc(now);
    }
}

// ---------------------------------------------------------------------------
// Human gate — Tauri commands (the ONLY approval path is GUI-invoked).
// ---------------------------------------------------------------------------

/// Free disk + worktree count for a repo, surfaced in the gate so the human
/// approves with resource context.
#[derive(Debug, Clone, Serialize)]
pub struct GateResourceInfo {
    pub worktree_count: usize,
    pub free_disk_bytes: u64,
    pub soft_cap: u32,
    /// True when `worktree_count >= soft_cap` (and `soft_cap > 0`).
    pub over_soft_cap: bool,
}

fn free_disk_bytes(path: &std::path::Path) -> u64 {
    // statvfs: f_bavail (blocks available to non-root) * f_frsize.
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return 0;
    };
    // SAFETY: `stat` is zeroed and only read after a successful (0) return;
    // `c_path` is a valid NUL-terminated string for the duration of the call.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            stat.f_bavail.saturating_mul(stat.f_frsize)
        } else {
            0
        }
    }
}

fn resource_info(repo_path: &std::path::Path, soft_cap: u32) -> GateResourceInfo {
    let worktree_count = crate::worktree::list_worktrees(repo_path)
        .map(|v| v.len())
        .unwrap_or(0);
    GateResourceInfo {
        worktree_count,
        free_disk_bytes: free_disk_bytes(repo_path),
        soft_cap,
        over_soft_cap: soft_cap > 0 && worktree_count as u32 >= soft_cap,
    }
}

/// A pending request enriched with live resource context for the gate UI.
#[derive(Debug, Clone, Serialize)]
pub struct GateRequestView {
    #[serde(flatten)]
    pub request: PendingWorktreeRequest,
    pub resources: GateResourceInfo,
}

/// List the pending requests (enriched with live count + free disk).
#[tauri::command]
pub fn list_worktree_requests(
    gate: tauri::State<'_, WorktreeGateState>,
) -> Result<Vec<GateRequestView>, String> {
    let soft_cap = crate::config::Config::load()
        .agent_spawned_worktrees
        .soft_worktree_cap;
    Ok(gate
        .list_pending()
        .into_iter()
        .map(|request| {
            let resources = resource_info(&request.repo_path, soft_cap);
            GateRequestView { request, resources }
        })
        .collect())
}

/// Deny a pending request (the requester receives a `denied` outcome).
#[tauri::command]
pub fn deny_worktree_request(
    app: AppHandle,
    request_id: String,
    gate: tauri::State<'_, WorktreeGateState>,
    agents: tauri::State<'_, AgentRuntimeState>,
) -> Result<(), String> {
    let Some(req) = gate.take_pending(&request_id) else {
        return Err("request is no longer pending".into());
    };
    gate.resolve(&request_id, WorktreeRequestState::Denied);
    let bridge = AppBridge::new(app);
    notify_outcome(
        &gate,
        &bridge,
        &agents,
        &req.requesting_session,
        &request_id,
        &WorktreeRequestState::Denied,
    );
    Ok(())
}

/// Approve a pending request: create the worktree session and hand it to the
/// user. This is the SOLE approval entry point and is GUI-only — no MCP or
/// permission-mode handler can reach it. Optional `edited_prompt`/`edited_branch`
/// apply the human's edits before creation.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — a struct breaks the JS call shape.
pub fn approve_worktree_request(
    app: AppHandle,
    request_id: String,
    edited_prompt: Option<String>,
    edited_branch: Option<String>,
    edited_preset: Option<String>,
    edited_agent: Option<String>,
    gate: tauri::State<'_, WorktreeGateState>,
    db: tauri::State<'_, crate::db::SharedDb>,
    agents: tauri::State<'_, AgentRuntimeState>,
    plan_watcher: tauri::State<'_, crate::agents::claude_code::plan::SharedPlanWatcher>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<crate::models::Session, String> {
    // Atomically claim the request. Losing the race (timeout/cancel/already
    // approved) means it is gone — never proceed to create.
    let Some(req) = gate.take_pending(&request_id) else {
        return Err("request is no longer pending".into());
    };

    let bridge = AppBridge::new(app);

    // Re-check the kill-switch at approve time: if the user disabled the feature
    // while this request sat pending, refuse rather than create — the toggle is
    // a true halt, not just an enqueue gate.
    if !crate::config::Config::load()
        .agent_spawned_worktrees
        .enabled
    {
        let reason = "feature disabled before approval".to_string();
        gate.resolve(
            &request_id,
            WorktreeRequestState::Failed {
                reason: reason.clone(),
            },
        );
        notify_outcome(
            &gate,
            &bridge,
            &agents,
            &req.requesting_session,
            &request_id,
            &WorktreeRequestState::Failed {
                reason: reason.clone(),
            },
        );
        return Err(reason);
    }
    let result = match &req.kind {
        RequestKind::Create => build_worktree_session(
            &req,
            edited_prompt,
            edited_branch,
            edited_preset,
            edited_agent,
            &db,
            &agents,
            &plan_watcher,
        ),
        RequestKind::Resume { target_session_id } => resume_session(
            &req,
            target_session_id,
            edited_prompt,
            &db,
            &agents,
            &plan_watcher,
        ),
    };
    let built = result.and_then(|(session, text)| {
        // Resume with no message yields empty text — nothing to queue.
        if !text.trim().is_empty() {
            crate::pty::queue_session_prompt(pty, session.id.clone(), text)?;
        }
        Ok(session)
    });
    match built {
        Ok(session) => {
            gate.resolve(
                &request_id,
                WorktreeRequestState::Approved {
                    session_id: session.id.clone(),
                },
            );
            notify_outcome(
                &gate,
                &bridge,
                &agents,
                &req.requesting_session,
                &request_id,
                &WorktreeRequestState::Approved {
                    session_id: session.id.clone(),
                },
            );
            Ok(session)
        }
        Err(reason) => {
            gate.resolve(
                &request_id,
                WorktreeRequestState::Failed {
                    reason: reason.clone(),
                },
            );
            notify_outcome(
                &gate,
                &bridge,
                &agents,
                &req.requesting_session,
                &request_id,
                &WorktreeRequestState::Failed {
                    reason: reason.clone(),
                },
            );
            Err(reason)
        }
    }
}

/// Create the worktree + session row + queue the first prompt. Mirrors
/// `clickup_spawn_worktree_with_task`. On a create-then-DB-fail, rolls back the
/// just-created worktree so no orphan dir is left.
#[allow(clippy::too_many_arguments)] // edits map 1:1 to the gate's editable fields.
fn build_worktree_session(
    req: &PendingWorktreeRequest,
    edited_prompt: Option<String>,
    edited_branch: Option<String>,
    edited_preset: Option<String>,
    edited_agent: Option<String>,
    db: &crate::db::SharedDb,
    agents: &AgentRuntimeState,
    plan_watcher: &crate::agents::claude_code::plan::SharedPlanWatcher,
) -> Result<(crate::models::Session, String), String> {
    let ts = now_secs();
    let prompt = edited_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&req.prompt);
    let branch_base = edited_branch
        .as_deref()
        .or(req.branch_name.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("agent worktree");

    // The human may downgrade (or change) the requested permission preset at the
    // gate — e.g. bypass → auto. Apply the edit over the requested LaunchOptions.
    let launch_options = {
        let mut lo = req.launch_options.clone();
        if let Some(preset_str) = edited_preset.as_deref().filter(|s| !s.is_empty())
            && let Ok(preset) = serde_json::from_value::<crate::models::PermissionPreset>(
                Value::String(preset_str.to_string()),
            )
        {
            lo.get_or_insert_with(Default::default).permission_preset = preset;
        }
        lo.filter(|lo| !lo.is_noop())
    };

    let (session, text) = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let repo_path = guard
            .workspace_repo_path(&req.workspace_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or("workspace not found")?;
        if !crate::worktree::is_git_repo(&repo_path) {
            return Err("workspace is not a git repository".into());
        }

        let slug = crate::commands::derive_worktree_slug(branch_base, ts);
        let worktree_dir = repo_path.join(".worktrees").join("cluihud").join(&slug);
        if worktree_dir.exists() {
            return Err(format!(
                "a worktree already exists for slug '{slug}' — edit the branch and retry"
            ));
        }
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;

        // Resolve the agent: the human's gate edit wins, else the requested one
        // (validated at request time), else the project/default resolution
        // (mirrors create_session). An empty / "default" edit means "don't
        // override — keep the requested-or-project default".
        let agent_id = edited_agent
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "default")
            .or(req.agent.as_deref())
            .and_then(|s| crate::agents::AgentId::new(s).ok())
            .unwrap_or_else(|| {
                let cfg = crate::config::Config::load();
                cfg.resolve_agent_for_project(&repo_path)
                    .as_deref()
                    .and_then(|s| crate::agents::AgentId::new(s).ok())
                    .unwrap_or_else(crate::agents::AgentId::claude_code)
            });

        let text = crate::pty::sanitize_for_pty(prompt);
        let session = crate::models::Session {
            id: format!(
                "{}-{ts}",
                req.workspace_id.chars().take(6).collect::<String>()
            ),
            name: branch_base.to_string(),
            workspace_id: req.workspace_id.clone(),
            worktree_path: Some(wt_path.clone()),
            worktree_branch: Some(format!("cluihud/{slug}")),
            merge_target: None,
            status: crate::models::SessionStatus::Idle,
            created_at: ts,
            updated_at: ts,
            agent_id: agent_id.as_str().to_string(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options,
            env_shells: Vec::new(),
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
            active_linear_issue_id: None,
            pinned_linear_issue_ids: Vec::new(),
        };
        // Spawn-failure rollback: the worktree exists now; if the DB insert
        // fails, remove the just-created worktree before bailing (no orphan).
        if let Err(e) = guard.create_session(&session) {
            let reason = format!("{e:#}");
            match crate::worktree::remove_worktree(&repo_path, &wt_path) {
                Ok(()) => {
                    return Err(format!(
                        "session create failed ({reason}); worktree rolled back"
                    ));
                }
                Err(rb) => {
                    return Err(format!(
                        "session create failed ({reason}); ROLLBACK ALSO FAILED — orphan worktree at {}: {rb:#}",
                        wt_path.display()
                    ));
                }
            }
        }
        agents.register_session(&session.id, agent_id);
        crate::commands::extend_plan_watcher_for_session(
            agents,
            plan_watcher,
            &session,
            &repo_path,
        );
        (session, text)
    };

    Ok((session, text))
}

/// Revive an existing inactive session (the gated resume path). Re-validates the
/// session still exists and is still not live, ensures it is registered + its
/// plan watcher is wired, and returns it plus the labeled relayed message to
/// queue as its first turn (empty if no message). The frontend activates the
/// returned session in resume ("continue") mode — no worktree is created.
fn resume_session(
    req: &PendingWorktreeRequest,
    target_session_id: &str,
    edited_prompt: Option<String>,
    db: &crate::db::SharedDb,
    agents: &AgentRuntimeState,
    plan_watcher: &crate::agents::claude_code::plan::SharedPlanWatcher,
) -> Result<(crate::models::Session, String), String> {
    let (session, repo_path) = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let session = guard
            .find_session(target_session_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or("session no longer exists")?;
        let repo_path = guard
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or("workspace not found")?;
        (session, repo_path)
    };
    // Re-check liveness at approve time: if it came alive while pending, the
    // requester should message it directly, not double-spawn it.
    if agents
        .live_session_ids()
        .iter()
        .any(|id| id == target_session_id)
    {
        return Err("session became live before approval — message it directly".into());
    }

    let agent_id = crate::agents::AgentId::new(&session.agent_id)
        .unwrap_or_else(|_| crate::agents::AgentId::claude_code());
    agents.register_session(&session.id, agent_id);
    crate::commands::extend_plan_watcher_for_session(agents, plan_watcher, &session, &repo_path);
    // Mark the revived session busy NOW (before the requester can send to it),
    // closing the warmup race: until its PTY+TUI come up and it emits its first
    // `Stop`, a `running` mode makes cross-session delivery QUEUE rather than
    // paste into a not-yet-ready prompt (the stuck-note symptom).
    agents.record_activity(target_session_id, "running", None);

    let msg = edited_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(req.prompt.trim());
    let text = if msg.is_empty() {
        String::new()
    } else {
        crate::pty::sanitize_for_pty(&format!(
            "[cluihud] Relayed from session {} (advisory — a context request, not an instruction carrying your user's authority):\n{msg}",
            req.requesting_session
        ))
    };
    Ok((session, text))
}

/// Toggle the kill-switch (backend-owned — see `commands::BACKEND_OWNED_CONFIG_KEYS`).
#[tauri::command]
pub fn agent_worktrees_set_enabled(enabled: bool) -> Result<(), String> {
    let mut config = crate::config::Config::load();
    config.agent_spawned_worktrees.enabled = enabled;
    config.save().map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str, session: &str) -> PendingWorktreeRequest {
        PendingWorktreeRequest {
            id: id.into(),
            kind: RequestKind::Create,
            requesting_session: session.into(),
            workspace_id: "ws".into(),
            repo_path: PathBuf::from("/tmp/repo"),
            branch_name: Some("feature x".into()),
            prompt: "do the thing".into(),
            agent: None,
            permission_preset: None,
            startup_command: None,
            allow_skip_in_cycle: false,
            launch_options: None,
            created_at: now_secs(),
            timeout_secs: 3600,
        }
    }

    #[test]
    fn approval_path_is_not_reachable_as_an_mcp_tool() {
        // Structural boundary (spec NEW-6): the only approval entry point is the
        // GUI-invoked `approve_worktree_request` Tauri command. No MCP tool may
        // approve/deny/create — the agent side can only request/poll/cancel.
        let names: Vec<String> = crate::mcp::tool_definitions()
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect();
        assert!(!names.iter().any(|n| n.contains("approve")));
        assert!(!names.iter().any(|n| n.contains("deny")));
        // The agent-facing surface is exactly request/resume/poll/cancel.
        assert!(names.contains(&"create_worktree_session".to_string()));
        assert!(names.contains(&"request_session_resume".to_string()));
        assert!(names.contains(&"get_worktree_request_status".to_string()));
        assert!(names.contains(&"cancel_worktree_request".to_string()));
    }

    #[test]
    fn pending_then_resolved_then_not_found() {
        let gate = WorktreeGateState::default();
        gate.insert_pending(req("r1", "s1"));
        assert!(matches!(gate.status("r1"), WorktreeRequestState::Pending));
        let taken = gate.take_pending("r1").unwrap();
        assert_eq!(taken.id, "r1");
        gate.resolve("r1", WorktreeRequestState::Denied);
        assert!(matches!(gate.status("r1"), WorktreeRequestState::Denied));
        // Unknown id is not_found.
        assert!(matches!(
            gate.status("nope"),
            WorktreeRequestState::NotFound
        ));
    }

    #[test]
    fn take_pending_is_mutually_exclusive() {
        let gate = WorktreeGateState::default();
        gate.insert_pending(req("r1", "s1"));
        // First caller wins; the second gets None and must abort.
        assert!(gate.take_pending("r1").is_some());
        assert!(gate.take_pending("r1").is_none());
    }

    #[test]
    fn timeout_atomically_purges_and_records() {
        let gate = WorktreeGateState::default();
        let mut r = req("r1", "s1");
        r.created_at = now_secs().saturating_sub(10_000); // well past a 3600s timeout
        r.timeout_secs = 3600;
        gate.insert_pending(r);
        let taken = gate.due_timeouts(now_secs());
        assert_eq!(taken.len(), 1);
        assert!(matches!(gate.status("r1"), WorktreeRequestState::TimedOut));
        // No longer approvable: take_pending returns None.
        assert!(gate.take_pending("r1").is_none());
    }

    #[test]
    fn approve_after_timeout_is_refused() {
        // The exact orphan-prevention invariant: a timeout that already purged
        // the entry means a later approve cannot claim it.
        let gate = WorktreeGateState::default();
        let mut r = req("r1", "s1");
        r.created_at = now_secs().saturating_sub(10_000);
        gate.insert_pending(r);
        assert_eq!(gate.due_timeouts(now_secs()).len(), 1);
        assert!(gate.take_pending("r1").is_none()); // approve loses the race
    }

    #[test]
    fn cancel_requires_ownership() {
        let gate = WorktreeGateState::default();
        gate.insert_pending(req("r1", "owner"));
        // A non-owner cannot cancel; the request stays pending.
        assert!(matches!(
            gate.cancel("r1", "intruder"),
            WorktreeRequestState::Pending
        ));
        assert!(matches!(gate.status("r1"), WorktreeRequestState::Pending));
        // The owner can.
        assert!(matches!(
            gate.cancel("r1", "owner"),
            WorktreeRequestState::Cancelled
        ));
        assert!(matches!(gate.status("r1"), WorktreeRequestState::Cancelled));
    }

    #[test]
    fn per_session_pending_count() {
        let gate = WorktreeGateState::default();
        gate.insert_pending(req("r1", "s1"));
        gate.insert_pending(req("r2", "s1"));
        gate.insert_pending(req("r3", "s2"));
        assert_eq!(gate.pending_count_for("s1"), 2);
        assert_eq!(gate.pending_count_for("s2"), 1);
    }

    #[test]
    fn ledger_gc_drops_old_entries() {
        let gate = WorktreeGateState::default();
        gate.insert_pending(req("r1", "s1"));
        gate.take_pending("r1");
        gate.resolve("r1", WorktreeRequestState::Denied);
        // GC far in the future drops it → not_found.
        gate.gc(now_secs() + LEDGER_RETENTION_SECS + 1);
        assert!(matches!(gate.status("r1"), WorktreeRequestState::NotFound));
    }

    fn test_ctx() -> DaemonContext {
        let db = Arc::new(std::sync::Mutex::new(
            crate::db::Database::open_in_memory().unwrap(),
        ));
        DaemonContext {
            db,
            agents: AgentRuntimeState::bootstrap().unwrap(),
            app_uid: 0,
            delivery: Arc::new(crate::mcp::delivery::NoopDelivery),
            worktree_gate: WorktreeGateState::default(),
        }
    }

    #[test]
    fn create_disabled_enqueues_nothing() {
        let ctx = test_ctx();
        let cfg = AgentWorktreesConfig {
            enabled: false,
            ..Default::default()
        };
        let v = create_worktree_session(&ctx, &cfg, "s1", "ws1", "do it", None, None, None);
        assert_eq!(v["status"], "disabled");
        assert!(ctx.worktree_gate.list_pending().is_empty());
    }

    #[test]
    fn create_empty_prompt_is_invalid_and_enqueues_nothing() {
        let ctx = test_ctx();
        let cfg = AgentWorktreesConfig {
            enabled: true,
            ..Default::default()
        };
        let v = create_worktree_session(&ctx, &cfg, "s1", "ws1", "   ", None, None, None);
        assert_eq!(v["status"], "invalid_request");
        assert!(ctx.worktree_gate.list_pending().is_empty());
    }

    #[test]
    fn create_unknown_workspace_is_invalid_and_enqueues_nothing() {
        let ctx = test_ctx();
        let cfg = AgentWorktreesConfig {
            enabled: true,
            ..Default::default()
        };
        let v = create_worktree_session(&ctx, &cfg, "s1", "nope", "do it", None, None, None);
        assert_eq!(v["status"], "invalid_workspace");
        assert!(ctx.worktree_gate.list_pending().is_empty());
    }

    #[test]
    fn resume_disabled_and_unknown_session() {
        let ctx = test_ctx();
        let off = AgentWorktreesConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(
            request_session_resume(&ctx, &off, "s1", "whatever", None)["status"],
            "disabled"
        );
        let on = AgentWorktreesConfig {
            enabled: true,
            ..Default::default()
        };
        // No such session in the empty in-memory DB.
        assert_eq!(
            request_session_resume(&ctx, &on, "s1", "ghost", None)["status"],
            "unknown_session"
        );
        assert!(ctx.worktree_gate.list_pending().is_empty());
    }

    #[test]
    fn undelivered_outcomes_queue_then_drain_once() {
        // The stranded-deny fix: an outcome that can't be pushed (requester
        // working) is queued, then taken exactly once on the idle drain.
        let gate = WorktreeGateState::default();
        gate.enqueue_outcome("s1", "denied".into());
        gate.enqueue_outcome("s1", "approved".into());
        gate.enqueue_outcome("s2", "timed out".into());
        let drained = gate.take_outcomes("s1");
        assert_eq!(drained, vec!["denied".to_string(), "approved".to_string()]);
        // Second drain is empty (delivered once, not re-delivered).
        assert!(gate.take_outcomes("s1").is_empty());
        // Other sessions are untouched.
        assert_eq!(gate.take_outcomes("s2"), vec!["timed out".to_string()]);
    }

    #[test]
    fn outcome_note_is_advisory_and_empty_for_non_outcomes() {
        let n = outcome_note(
            "r1",
            &WorktreeRequestState::Approved {
                session_id: "sess".into(),
            },
        );
        assert!(n.contains("advisory"));
        assert!(n.contains("sess"));
        assert!(outcome_note("r1", &WorktreeRequestState::Pending).is_empty());
        assert!(outcome_note("r1", &WorktreeRequestState::Cancelled).is_empty());
    }
}
