use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncBufReadExt;

use super::events::HookEvent;
use crate::agents::AgentId;
use crate::agents::state::AgentRuntimeState;
use crate::db::SharedDb;
use crate::plan_state::SharedPlanState;
use crate::platform::RejectionRateLimit;

/// Flat event structure the frontend expects.
#[derive(Clone, serde::Serialize)]
struct FrontendHookEvent {
    session_id: String,
    nergal_session_id: Option<String>,
    event_type: String,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
    stop_reason: Option<String>,
    transcript_path: Option<String>,
}

impl FrontendHookEvent {
    fn from_hook(event: &HookEvent) -> Self {
        let (session_id, event_type, tool_name, tool_input, stop_reason, transcript_path) =
            match event {
                HookEvent::SessionStart { session_id } => {
                    (session_id.clone(), "session_start", None, None, None, None)
                }
                HookEvent::SessionEnd { session_id } => {
                    (session_id.clone(), "session_end", None, None, None, None)
                }
                HookEvent::PreToolUse {
                    session_id,
                    tool_name,
                    tool_input,
                } => (
                    session_id.clone(),
                    "pre_tool_use",
                    Some(tool_name.clone()),
                    Some(tool_input.clone()),
                    None,
                    None,
                ),
                HookEvent::PostToolUse {
                    session_id,
                    tool_name,
                    tool_input,
                    ..
                } => (
                    session_id.clone(),
                    "post_tool_use",
                    Some(tool_name.clone()),
                    Some(tool_input.clone()),
                    None,
                    None,
                ),
                HookEvent::Stop {
                    session_id,
                    stop_reason,
                    transcript_path,
                    ..
                } => (
                    session_id.clone(),
                    "stop",
                    None,
                    None,
                    stop_reason.clone(),
                    transcript_path.clone(),
                ),
                HookEvent::TaskCompleted {
                    session_id,
                    task_subject,
                    ..
                } => (
                    session_id.clone(),
                    "task_completed",
                    task_subject.clone(),
                    None,
                    None,
                    None,
                ),
                HookEvent::UserPromptSubmit { session_id } => (
                    session_id.clone(),
                    "user_prompt_submit",
                    None,
                    None,
                    None,
                    None,
                ),
                HookEvent::TaskCreated {
                    session_id,
                    task_subject,
                    tool_input,
                    ..
                } => (
                    session_id.clone(),
                    "task_created",
                    task_subject.clone(),
                    Some(tool_input.clone()),
                    None,
                    None,
                ),
                HookEvent::PlanReview {
                    session_id,
                    tool_name,
                    tool_input,
                    ..
                } => (
                    session_id.clone(),
                    "plan_review",
                    Some(tool_name.clone()),
                    Some(tool_input.clone()),
                    None,
                    None,
                ),
                HookEvent::AskUser {
                    session_id,
                    tool_input,
                    ..
                } => (
                    session_id.clone(),
                    "ask_user",
                    None,
                    Some(tool_input.clone()),
                    None,
                    None,
                ),
                HookEvent::Notification {
                    session_id,
                    notification_type,
                    ..
                } => (
                    session_id.clone(),
                    "notification",
                    notification_type.clone(),
                    None,
                    None,
                    None,
                ),
                HookEvent::CwdChanged { session_id, .. } => {
                    (session_id.clone(), "cwd_changed", None, None, None, None)
                }
                HookEvent::FileChanged {
                    session_id,
                    file_path,
                    ..
                } => (
                    session_id.clone(),
                    "file_changed",
                    file_path.clone(),
                    None,
                    None,
                    None,
                ),
                HookEvent::PermissionDenied {
                    session_id,
                    tool_name,
                    tool_input,
                    reason,
                } => (
                    session_id.clone(),
                    "permission_denied",
                    tool_name.clone(),
                    Some(tool_input.clone()),
                    reason.clone(),
                    None,
                ),
                HookEvent::StatusLine { session_id, .. } => {
                    (session_id.clone(), "statusline", None, None, None, None)
                }
                HookEvent::AgentStatus { session_id, .. } => {
                    (session_id.clone(), "agent_status", None, None, None, None)
                }
            };
        Self {
            session_id,
            nergal_session_id: None,
            event_type: event_type.into(),
            tool_name,
            tool_input,
            stop_reason,
            transcript_path,
        }
    }
}

/// Starts the IPC server that receives hook events from the agent CLI. Bound
/// through `PlatformListener` (Unix socket / Windows named pipe); the peer is
/// gated by `PeerIdentity::matches_current_process()` on both platforms.
pub async fn start_hook_server(
    socket_path: &Path,
    app: AppHandle,
    db: SharedDb,
    plan_state: SharedPlanState,
    agent_state: AgentRuntimeState,
) -> Result<()> {
    use crate::platform::PlatformListener;

    // `PlatformListener::bind` handles the stale-endpoint probe + owner-only
    // permissions (0600 behind the 0700 dir on Unix; owner-only security
    // descriptor on Windows). A live peer surfaces as `AddrInUse`, which means
    // an instance is already serving → return Ok and let it serve.
    let listener = match PlatformListener::bind(socket_path) {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            tracing::warn!(
                ipc_event = "bind_deferred",
                path = %socket_path.display(),
                "live hook endpoint found at startup; deferring bind"
            );
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    tracing::info!("hook server listening on {}", socket_path.display());

    let rejection_log: Arc<RejectionRateLimit> = Arc::new(RejectionRateLimit::new());

    loop {
        // A single bad connection (I/O error, SID-extraction failure on
        // Windows) must not kill the server — log and keep accepting.
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(ipc_event = "accept_error", error = %e, "hook accept failed");
                continue;
            }
        };

        // Peer gate: reject + rate-limited log any connection from a foreign
        // principal BEFORE reading any line. Defence-in-depth behind the
        // per-user endpoint isolation (0700 dir on Unix, owner-only SD on
        // Windows). The rejection_log coalesces so a spammer cannot fill disk.
        if !peer.matches_current_process() {
            rejection_log.report(&peer.display(), "hook");
            continue; // stream dropped here → connection closed without reading
        }

        let app = app.clone();
        let db = db.clone();
        let plan_state = plan_state.clone();
        let agent_state = agent_state.clone();

        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stream);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                // Peek at the `kind` discriminator so control messages
                // (e.g. {"kind":"control","op":"rescan_agents"}) don't fall
                // into the hook-event parser. Missing `kind` is treated as
                // `hook_event` for backward compat with installed CC hook
                // pipelines (Decision 14).
                let parsed = serde_json::from_str::<serde_json::Value>(&line).ok();
                let kind = parsed
                    .as_ref()
                    .and_then(|v| v.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("hook_event");

                if kind == "control" {
                    process_control_message(&app, &agent_state, parsed.as_ref()).await;
                    continue;
                }

                // Treat empty string the same as missing: an empty NERGAL_SESSION_ID
                // env var is functionally equivalent to no env var, and registering
                // "" as a session would silently swallow every later event for it.
                let nergal_sid = parsed.as_ref().and_then(|v| {
                    v.get("nergal_session_id")
                        .and_then(|s| s.as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                });

                match serde_json::from_str::<HookEvent>(&line) {
                    Ok(event) => {
                        process_event(
                            &app,
                            &event,
                            &db,
                            &plan_state,
                            &agent_state,
                            nergal_sid.as_deref(),
                        );
                    }
                    Err(e) => {
                        tracing::warn!("failed to parse hook event: {e}");
                    }
                }
            }
        });
    }
}

/// Handle a `kind=control` message off the hook socket. Today the only op is
/// `rescan_agents`: re-runs the registry's filesystem detection and emits an
/// `agents:detected` event to the frontend with the fresh list. Future ops
/// (shutdown, status, …) extend the match.
async fn process_control_message(
    app: &AppHandle,
    agent_state: &AgentRuntimeState,
    payload: Option<&serde_json::Value>,
) {
    let op = payload
        .and_then(|v| v.get("op"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match op {
        "rescan_agents" => {
            let detections = agent_state.registry.scan().await;
            #[derive(Clone, serde::Serialize)]
            struct Entry {
                id: String,
                installed: bool,
                binary_path: Option<String>,
                config_path: Option<String>,
                version: Option<String>,
            }
            let payload: Vec<Entry> = detections
                .into_iter()
                .map(|(id, det)| Entry {
                    id: id.as_str().to_string(),
                    installed: det.installed,
                    binary_path: det.binary_path.map(|p| p.display().to_string()),
                    config_path: det.config_path.map(|p| p.display().to_string()),
                    version: det.version,
                })
                .collect();
            let _ = app.emit("agents:detected", payload);
            tracing::info!("rescan-agents: emitted agents:detected");
        }
        other => {
            tracing::warn!(op = other, "unknown control op; ignoring");
        }
    }
}

/// Drains the EventSink that adapters feed (OpenCode SSE, Pi JSONL tail).
/// Each translated [`HookEvent`] is routed through [`process_event`] using
/// the event's own session_id as the nergal_session_id (adapters embed it
/// when they construct the event). The task runs for the lifetime of the
/// app and exits when all senders drop.
pub fn spawn_adapter_event_consumer(
    app: AppHandle,
    db: SharedDb,
    plan_state: SharedPlanState,
    agent_state: AgentRuntimeState,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<HookEvent>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            let csid = event.session_id().to_string();
            process_event(&app, &event, &db, &plan_state, &agent_state, Some(&csid));
        }
    });
}

/// #2 session log: resolve the workspace obsidian config + session for the
/// active hook, but only when the session_log channel is set. None → skip.
fn session_log_cfg(
    db: &SharedDb,
    csid: Option<&str>,
) -> Option<(
    crate::obsidian::config::ResolvedObsidianConfig,
    crate::models::Session,
)> {
    let csid = csid?;
    let guard = db.lock().ok()?;
    let session = guard.find_session(csid).ok()??;
    let cfg =
        crate::obsidian::config::resolve(&session.workspace_id, |w| guard.get_obsidian_config(w))
            .ok()?;
    cfg.session_log_path.as_deref().filter(|s| !s.is_empty())?;
    Some((cfg, session))
}

/// One activity line per loggable event; None for events the log skips.
/// SessionStart/SessionEnd are special-cased in their match arms.
fn session_log_line(event: &HookEvent) -> Option<String> {
    fn file_of(input: &serde_json::Value) -> Option<&str> {
        input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(|v| v.as_str())
    }
    match event {
        HookEvent::PreToolUse {
            tool_name,
            tool_input,
            ..
        } => Some(match tool_name.as_str() {
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Update" => file_of(tool_input)
                .map_or_else(|| format!("Tool {tool_name}"), |f| format!("Edit {f}")),
            "Read" => {
                file_of(tool_input).map_or_else(|| "Read".to_string(), |f| format!("Read {f}"))
            }
            other => format!("Tool {other}"),
        }),
        HookEvent::Stop { stop_reason, .. } => stop_reason
            .as_deref()
            .filter(|r| !r.is_empty())
            .map(|r| format!("Stop (reason: {r})")),
        HookEvent::TaskCreated { task_subject, .. } => task_subject
            .as_deref()
            .map(|s| format!("Task created: \"{s}\"")),
        HookEvent::TaskCompleted { task_subject, .. } => task_subject
            .as_deref()
            .map(|s| format!("Task completed: \"{s}\"")),
        HookEvent::UserPromptSubmit { .. } => Some("Prompt submitted".to_string()),
        HookEvent::PlanReview { .. } => Some("Plan ready".to_string()),
        HookEvent::FileChanged {
            file_path,
            event_type,
            ..
        } => file_path
            .as_deref()
            .map(|f| format!("{} {f}", event_type.as_deref().unwrap_or("Changed"))),
        HookEvent::PermissionDenied {
            tool_name, reason, ..
        } => Some(format!(
            "Permission denied: {} — {}",
            tool_name.as_deref().unwrap_or("?"),
            reason.as_deref().unwrap_or("—")
        )),
        _ => None,
    }
}

fn model_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static C: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// The model is only known once StatusLine/AgentStatus arrives (after
/// SessionStart), so it lands in the log footer, not the header.
fn cache_model(csid: Option<&str>, model_name: &Option<String>) {
    if let (Some(csid), Some(m)) = (csid, model_name.as_deref())
        && !m.is_empty()
        && let Ok(mut c) = model_cache().lock()
    {
        c.insert(csid.to_string(), m.to_string());
    }
}

fn cached_model(csid: &str) -> Option<String> {
    model_cache().lock().ok().and_then(|c| c.get(csid).cloned())
}

/// Read phase of the log footer: whether the session_log channel is active,
/// and (if so) the completed-task count. `None` short-circuits the write
/// phase (channel unset) without touching the DB.
pub(crate) fn gather_footer_tasks_done(
    db: &crate::db::Database,
    cfg: &crate::obsidian::config::ResolvedObsidianConfig,
    csid: &str,
) -> Option<usize> {
    cfg.session_log_path.as_deref().filter(|s| !s.is_empty())?;
    Some(
        db.get_visible_tasks(csid)
            .map(|ts| {
                ts.iter()
                    .filter(|t| matches!(t.status, crate::tasks::TaskStatus::Completed))
                    .count()
            })
            .unwrap_or(0),
    )
}

/// Write phase: no `&Database` — the model comes from the in-process cache.
/// Dedup contract: callers MUST have already won `claim_finalization(csid)`
/// before invoking this — it appends the footer unconditionally, so a second
/// unclaimed call would write a duplicate.
pub(crate) fn write_footer(
    cfg: &crate::obsidian::config::ResolvedObsidianConfig,
    csid: &str,
    tasks_done: usize,
) {
    let _ = crate::obsidian::channels::SessionLogWriter::end_session(
        cfg,
        cached_model(csid).as_deref(),
        &[],
        tasks_done,
    );
}

/// Session obsidian finalization: write the #2 log footer and, if the moc
/// channel is set, snapshot the session (detached runner, or inline when the
/// startup probe found bg processing unavailable). Deduped so the SessionEnd
/// hook and the PTY-EOF trigger produce exactly one snapshot per session.
pub(crate) fn finalize_session_obsidian(db: &SharedDb, csid: Option<&str>) {
    let Some(csid) = csid else { return };
    if !crate::obsidian::post_session::claim_finalization(csid) {
        return;
    }
    // Gather every DB-derived input under one guard, then drop it before the
    // footer/MOC file I/O and (in the bg branch) the detached spawn — both
    // branches used to hold the guard asymmetrically; now neither does.
    let (session, cfg, footer_tasks_done, has_moc, bg, moc_inputs) = {
        let guard = match db.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let session = match guard.find_session(csid) {
            Ok(Some(s)) => s,
            _ => return,
        };
        let cfg = match crate::obsidian::config::resolve(&session.workspace_id, |w| {
            guard.get_obsidian_config(w)
        }) {
            Ok(c) => c,
            Err(_) => return,
        };
        let footer_tasks_done = gather_footer_tasks_done(&guard, &cfg, csid);
        let has_moc = cfg.moc_path.as_deref().filter(|s| !s.is_empty()).is_some();
        let bg = has_moc && crate::obsidian::post_session::runner_available();
        let moc_inputs = if has_moc && !bg {
            crate::obsidian::moc::MocBuilder::gather(&session.id, &cfg, &guard)
                .ok()
                .flatten()
        } else {
            None
        };
        (session, cfg, footer_tasks_done, has_moc, bg, moc_inputs)
    }; // guard dropped here

    if let Some(tasks_done) = footer_tasks_done {
        write_footer(&cfg, csid, tasks_done);
    }
    if has_moc {
        if bg {
            let _ = crate::obsidian::post_session::write_marker(
                &session.id,
                &session.workspace_id,
                &session.agent_id,
                "SessionEnd",
            );
            let _ = crate::obsidian::post_session::spawn_runner_detached();
        } else if let Some(inputs) = moc_inputs
            && let Ok(Some(moc)) = inputs.render_and_write(&cfg)
        {
            let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc, &cfg);
        }
    }
    if let Ok(mut c) = model_cache().lock() {
        c.remove(csid);
    }
}

/// CC's candidate plansDirectory paths for a session, most-specific first.
/// CC writes the plan markdown to one of these: modern CC defaults to the
/// home-global `~/.claude/plans` (the only one that exists on Windows), older
/// CC used the project-local `<cwd>/.claude/plans`. Resolving the session's cwd
/// first preserves the project-local case; the home default covers modern CC.
/// A workspace-level `plans_dir` override is prepended (additive: where to look,
/// not where CC writes). Relative `plansDirectory` values are searched under
/// both cwd and home so resolution is robust across agent host OSes.
fn cc_plan_dirs(db: &SharedDb, nergal_session_id: Option<&str>) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(csid) = nergal_session_id
        && let Ok(db_guard) = db.lock()
        && let Ok(Some(session)) = db_guard.find_session(csid)
    {
        let cwd = session.worktree_path.or_else(|| {
            db_guard
                .workspace_repo_path(&session.workspace_id)
                .ok()
                .flatten()
        });

        // Workspace-level override is prepended — additive, searched first. A
        // relative override resolves against the workspace cwd (its natural base).
        if let Ok(Some(ov)) = db_guard.get_workspace_plans_dir(&session.workspace_id) {
            let p = PathBuf::from(&ov);
            let abs = if p.is_absolute() {
                p
            } else if let Some(ref cwd) = cwd {
                cwd.join(&ov)
            } else {
                p
            };
            dirs.push(abs);
        }

        if let Some(ref cwd) = cwd {
            for d in crate::agents::claude_code::plans_path::candidate_dirs(cwd, home.as_deref()) {
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        }
    }

    if let Some(ref h) = home {
        let global = h.join(".claude").join("plans");
        if !dirs.contains(&global) {
            dirs.push(global);
        }
    }

    dirs
}

/// Resolve the plan to surface for an ExitPlanMode / PlanReview event, preferring
/// the exact source over the newest-mtime heuristic:
///   1. `tool_input["plan_path"]` — an explicit path, honored if ever provided.
///   2. `tool_input["plan"]` — the markdown CC delivers inline in the hook
///      payload: the precise text the agent emitted, with no filesystem race.
///      Its backing file is located by *content identity* so the edit/re-inject
///      round-trip writes back to the right plan; if CC hasn't flushed the file
///      yet, the inline content is still shown with the newest file as a
///      best-effort writable path.
///   3. Legacy fallback: newest `.md` across the candidate dirs.
fn resolve_active_plan(
    tool_input: &serde_json::Value,
    dirs: &[PathBuf],
) -> Option<(PathBuf, String)> {
    use crate::agents::claude_code::plan::{PlanManager, find_plan_file_by_content};

    let newest = |dirs: &[PathBuf]| -> Option<PathBuf> {
        dirs.iter().find_map(|d| {
            PlanManager::new(d.clone())
                .find_latest_plan()
                .ok()
                .flatten()
        })
    };

    if let Some(path) = tool_input.get("plan_path").and_then(|v| v.as_str()) {
        let path = PathBuf::from(path);
        let content = std::fs::read_to_string(&path).ok()?;
        return Some((path, content));
    }

    if let Some(content) = tool_input
        .get("plan")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        for dir in dirs {
            if let Some(path) = find_plan_file_by_content(dir, content) {
                return Some((path, content.to_string()));
            }
        }
        let path = newest(dirs).unwrap_or_default();
        return Some((path, content.to_string()));
    }

    let path = newest(dirs)?;
    let content = std::fs::read_to_string(&path).ok()?;
    Some((path, content))
}

fn process_event(
    app: &AppHandle,
    event: &HookEvent,
    db: &SharedDb,
    plan_state: &SharedPlanState,
    agent_state: &AgentRuntimeState,
    nergal_session_id: Option<&str>,
) {
    if nergal_session_id.is_none() {
        tracing::warn!(
            event_type = ?std::any::type_name_of_val(event),
            "dropping hook event: missing nergal_session_id (env var not propagated through PTY shell?)",
        );
        return;
    }

    // Capture background tasks / crons + the closing summary (CC v2.1.150+) so
    // the MCP descriptor can surface them. Every Stop overwrites: empty arrays
    // must clear a prior snapshot (a finished/killed bg task has to stop showing
    // as running), so there is deliberately no "skip empty payloads" gate.
    if let HookEvent::Stop {
        session_id,
        background_tasks,
        session_crons,
        last_assistant_message,
        transcript_path,
        ..
    } = event
    {
        let csid = nergal_session_id.unwrap_or(session_id);
        agent_state.set_session_background(csid, background_tasks.clone(), session_crons.clone());
        agent_state.set_session_last_message(csid, last_assistant_message.clone());
        // Pull-based summaries (Revision 1): `Stop` only writes the cheap,
        // LLM-free pull-marker (transcript path + activity timestamp). Actual
        // generation is triggered lazily by `get_session` on the read path.
        // Written unconditionally (independent of the summary opt-in) since the
        // marker is just a pointer; `get_session` gates on the backend.
        //
        // Guard the path: the marker drives an on-demand file read fed to an LLM
        // backend, so reject anything that is not a `.jsonl` transcript. All four
        // adapters write `.jsonl` transcripts, so this never rejects a real one;
        // it blocks a crafted same-uid hook payload from steering the summarizer
        // at an arbitrary file (e.g. a secret) for exfiltration.
        if let Some(path) = transcript_path.as_deref()
            && path.ends_with(".jsonl")
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if let Ok(guard) = db.lock()
                && let Err(e) = guard.set_session_transcript(csid, path, now)
            {
                tracing::warn!(session_id = %csid, "persisting pull-marker failed: {e:#}");
            }
        }
    }

    // Resolve the owning adapter for this session (cache → DB-fallback path
    // lands once the DB schema carries agent_id; until then unknown sessions
    // are tagged claude-code defensively to preserve current behavior). Drop
    // events whose session is fully unknown — they are orphans.
    if let Some(csid) = nergal_session_id
        && agent_state.resolve(csid).is_none()
    {
        tracing::warn!(
            nergal_session_id = %csid,
            event_type = ?std::any::type_name_of_val(event),
            "hook event for session not in agent cache; assuming claude-code (foundation transitional)"
        );
        agent_state.register_session(csid, AgentId::claude_code());
    }

    // Mirror the frontend `modeMapAtom` into the runtime side-map so the MCP
    // descriptor reports live mode + last_activity instead of the frozen DB
    // row. Telemetry-only events (`mcp_mode` → None) leave the prior state.
    if let Some(csid) = nergal_session_id {
        if let Some(mode) = event.mcp_mode() {
            agent_state.record_activity(csid, mode, event.waiting_for().as_deref());
        }
        // Touched files come from PreToolUse: it is registered unmatched so it
        // fires for every tool (Read included), whereas the PostToolUse hook is
        // matcher-filtered to writes — relying on it would miss reads. Carry the
        // tool name so the descriptor shows how each path was touched.
        let tool_ctx = match event {
            HookEvent::PreToolUse {
                tool_name,
                tool_input,
                ..
            }
            | HookEvent::PostToolUse {
                tool_name,
                tool_input,
                ..
            } => Some((tool_name.as_str(), tool_input)),
            HookEvent::PermissionDenied {
                tool_name,
                tool_input,
                ..
            } => Some((tool_name.as_deref().unwrap_or("tool"), tool_input)),
            _ => None,
        };
        if let Some((tool, input)) = tool_ctx
            && let Some(path) = file_path_from_tool_input(input)
        {
            agent_state.record_touched_file(csid, path, tool);
        }
    }

    // Cross-session delivery liveness drain (cross-session-messaging, finding 3):
    // a `Stop` is the working→idle transition, so deliver any messages that
    // queued while this session was working — for ALL agents, not just CC.
    // `drain_idle` gates on the kill-switch (passed in) and only wakes when the
    // pending queue is non-empty, so this is a no-op when the feature is off or
    // nothing queued. Gated to `Stop` (not `SessionEnd`) so a teardown never
    // wakes a dying PTY.
    if let Some(csid) = nergal_session_id
        && matches!(event, HookEvent::Stop { .. })
    {
        let bridge = crate::mcp::delivery::AppBridge::new(app.clone());
        let cross_session_enabled = crate::config::Config::load().cross_session.enabled;
        // At most ONE PTY paste per idle transition: two bracketed pastes
        // back-to-back race each other's `\r` submit (the second lands before
        // the first's turn starts), leaving a note stuck in the prompt. So the
        // worktree-outcome drain only runs when the cross-session drain pasted
        // nothing this Stop; otherwise it waits for the next idle.
        let delivered = crate::mcp::delivery::drain_idle(db, &bridge, csid, cross_session_enabled);
        if delivered == 0
            && let Some(gate) = app.try_state::<crate::mcp::worktree_sessions::WorktreeGateState>()
        {
            crate::mcp::worktree_sessions::drain_worktree_outcomes(&gate, &bridge, csid);
        }
    }

    let mut frontend_event = FrontendHookEvent::from_hook(event);
    frontend_event.nergal_session_id = nergal_session_id.map(String::from);
    let _ = app.emit("hook:event", &frontend_event);

    // #2 continuous session log — best-effort append for loggable events.
    if let Some(line) = session_log_line(event)
        && let Some((cfg, _)) = session_log_cfg(db, nergal_session_id)
    {
        let _ = crate::obsidian::channels::SessionLogWriter::append_event(&cfg, &line);
    }

    match event {
        HookEvent::SessionStart { session_id } => {
            #[derive(Clone, serde::Serialize)]
            struct SessionStartPayload {
                id: String,
                active: bool,
                cwd: Option<String>,
            }
            let cwd = std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string());
            let _ = app.emit(
                "session:start",
                SessionStartPayload {
                    id: session_id.clone(),
                    active: true,
                    cwd,
                },
            );

            if let Some((cfg, session)) = session_log_cfg(db, nergal_session_id) {
                // Worktree sessions log their worktree; direct sessions fall back
                // to the workspace repo path so Cwd is never a bare "?".
                let log_cwd = session
                    .worktree_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| {
                        db.lock()
                            .ok()
                            .and_then(|g| {
                                g.workspace_repo_path(&session.workspace_id).ok().flatten()
                            })
                            .map(|p| p.display().to_string())
                    });
                let model = nergal_session_id.and_then(cached_model);
                let _ = crate::obsidian::channels::SessionLogWriter::start_session(
                    &cfg,
                    &session.name,
                    &session.agent_id,
                    model.as_deref(),
                    log_cwd.as_deref(),
                );
            }
        }

        HookEvent::SessionEnd { session_id } => {
            let _ = app.emit("session:end", session_id);
            finalize_session_obsidian(db, nergal_session_id);
        }

        HookEvent::PreToolUse {
            tool_name,
            tool_input,
            session_id,
        } => {
            tracing::debug!("PreToolUse: tool_name={tool_name}");

            if tool_name == "ExitPlanMode"
                && let Ok(mut state) = plan_state.lock()
            {
                let mut dirs = cc_plan_dirs(db, nergal_session_id);
                let runtime_dir = state.get_or_create(session_id).plans_dir.clone();
                if !dirs.contains(&runtime_dir) {
                    dirs.push(runtime_dir);
                }

                if let Some((path, content)) = resolve_active_plan(tool_input, &dirs) {
                    state
                        .get_or_create(session_id)
                        .set_plan(path.clone(), content.clone());

                    #[derive(Clone, serde::Serialize)]
                    struct PlanReady {
                        path: String,
                        content: String,
                        session_id: String,
                    }
                    let _ = app.emit(
                        "plan:ready",
                        PlanReady {
                            path: path.display().to_string(),
                            content,
                            session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                        },
                    );
                }
            }
        }

        HookEvent::PostToolUse {
            tool_name,
            tool_input,
            session_id,
            ..
        } => {
            tracing::debug!("PostToolUse: tool_name={tool_name}");
            let csid = nergal_session_id.unwrap_or(session_id);
            if tool_name == "AskUserQuestion" {
                #[derive(Clone, serde::Serialize)]
                struct AskUserResolvedPayload {
                    session_id: String,
                }
                let _ = app.emit(
                    "ask:user-resolved",
                    AskUserResolvedPayload {
                        session_id: csid.to_string(),
                    },
                );
            }
            process_task_event(app, tool_name, tool_input, csid, db);
            process_file_event(app, tool_name, tool_input, csid);
        }

        HookEvent::Stop {
            session_id,
            transcript_path: Some(path),
            ..
        } => {
            let cost = crate::agents::claude_code::cost::parse_cost_from_transcript(
                &std::path::PathBuf::from(path),
            );

            let csid = nergal_session_id.unwrap_or(session_id);
            if let Ok(db_guard) = db.lock()
                && let Err(e) = db_guard.upsert_cost(csid, &cost)
            {
                tracing::warn!("upsert_cost failed for session {csid}: {e}");
            }

            #[derive(Clone, serde::Serialize)]
            struct CostUpdate {
                session_id: String,
                input_tokens: u64,
                output_tokens: u64,
                cache_read: u64,
                cache_write: u64,
                total_usd: f64,
            }
            let _ = app.emit(
                "cost:update",
                CostUpdate {
                    session_id: csid.to_string(),
                    input_tokens: cost.input_tokens,
                    output_tokens: cost.output_tokens,
                    cache_read: cost.cache_read_tokens,
                    cache_write: cost.cache_write_tokens,
                    total_usd: cost.total_usd,
                },
            );
        }

        HookEvent::Stop {
            transcript_path: None,
            ..
        } => {}

        HookEvent::TaskCompleted { .. } => {}

        HookEvent::TaskCreated {
            session_id,
            tool_input,
            ..
        } => {
            let csid = nergal_session_id.unwrap_or(session_id);
            process_task_event(app, "TaskCreate", tool_input, csid, db);
        }

        // Tolerated no-op: the send-gate was removed (2026-06-11), but a
        // stale `nergal hook send user-prompt` entry may keep firing until
        // `nergal hook setup` purges it from the user's settings.
        HookEvent::UserPromptSubmit { .. } => {}

        HookEvent::PlanReview {
            tool_input,
            session_id,
            fifo_path,
            ..
        } => {
            tracing::debug!("PlanReview: fifo_path={fifo_path}");

            // Hand the FIFO path to the CC adapter so future
            // adapter.submit_plan_decision() calls know where to unblock the
            // plan-review CLI. Today the production path still goes through
            // the legacy commands::submit_plan_decision (which receives the
            // FIFO from the frontend); this registration is the seam used
            // when the call site flips to the trait method.
            if let Some(csid) = nergal_session_id {
                agent_state
                    .claude_code
                    .register_pending_plan_fifo(csid, std::path::PathBuf::from(fifo_path));
            }

            if let Ok(mut state) = plan_state.lock() {
                let mut dirs = cc_plan_dirs(db, nergal_session_id);
                let runtime_dir = state.get_or_create(session_id).plans_dir.clone();
                if !dirs.contains(&runtime_dir) {
                    dirs.push(runtime_dir);
                }

                if let Some((path, content)) = resolve_active_plan(tool_input, &dirs) {
                    state
                        .get_or_create(session_id)
                        .set_plan(path.clone(), content.clone());

                    #[derive(Clone, serde::Serialize)]
                    struct PlanReady {
                        path: String,
                        content: String,
                        session_id: String,
                        decision_path: String,
                    }
                    let _ = app.emit(
                        "plan:ready",
                        PlanReady {
                            path: path.display().to_string(),
                            content,
                            session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                            decision_path: fifo_path.clone(),
                        },
                    );
                }
            }
        }

        HookEvent::CwdChanged { session_id, cwd } => {
            tracing::debug!("CwdChanged: cwd={cwd:?}");

            #[derive(Clone, serde::Serialize)]
            struct CwdChangedPayload {
                session_id: String,
                cwd: String,
            }
            if let Some(dir) = cwd {
                let _ = app.emit(
                    "cwd:changed",
                    CwdChangedPayload {
                        session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                        cwd: dir.clone(),
                    },
                );
            }
        }

        HookEvent::FileChanged {
            session_id,
            file_path,
            event_type,
        } => {
            tracing::debug!("FileChanged: path={file_path:?} type={event_type:?}");

            #[derive(Clone, serde::Serialize)]
            struct FileChangedPayload {
                session_id: String,
                path: String,
                change_type: String,
            }
            if let Some(path) = file_path {
                let _ = app.emit(
                    "file:changed",
                    FileChangedPayload {
                        session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                        path: path.clone(),
                        change_type: event_type.clone().unwrap_or_default(),
                    },
                );
            }
        }

        HookEvent::PermissionDenied {
            session_id,
            tool_name,
            reason,
            ..
        } => {
            tracing::info!("PermissionDenied: tool={tool_name:?} reason={reason:?}");

            #[derive(Clone, serde::Serialize)]
            struct PermissionDeniedPayload {
                session_id: String,
                tool_name: Option<String>,
                reason: Option<String>,
            }
            let _ = app.emit(
                "permission:denied",
                PermissionDeniedPayload {
                    session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                    tool_name: tool_name.clone(),
                    reason: reason.clone(),
                },
            );
        }

        HookEvent::StatusLine {
            session_id,
            model_id,
            model_name,
            context_used_pct,
            context_window_size,
            rate_5h_pct,
            rate_5h_resets_at,
            rate_7d_pct,
            rate_7d_resets_at,
            ..
        } => {
            // Legacy CC statusline payload — translate into the agent-agnostic
            // AgentStatus shape so the frontend has a single listener. Kept
            // for back-compat while installed statusline scripts still emit
            // the old hook_event_name.
            cache_model(nergal_session_id, model_name);
            emit_agent_status(
                app,
                AgentStatusEmit {
                    session_id: nergal_session_id.unwrap_or(session_id),
                    agent_id: Some("claude-code"),
                    model_id: model_id.clone(),
                    model_name: model_name.clone(),
                    session_started_at: None,
                    context_used_pct: *context_used_pct,
                    context_window_size: *context_window_size,
                    rate_5h_pct: *rate_5h_pct,
                    rate_5h_resets_at: *rate_5h_resets_at,
                    rate_7d_pct: *rate_7d_pct,
                    rate_7d_resets_at: *rate_7d_resets_at,
                    effort_level: None,
                },
            );
        }

        HookEvent::AgentStatus {
            session_id,
            agent_id,
            model_id,
            model_name,
            session_started_at,
            context_used_pct,
            context_window_size,
            rate_5h_pct,
            rate_5h_resets_at,
            rate_7d_pct,
            rate_7d_resets_at,
            effort_level,
        } => {
            cache_model(nergal_session_id, model_name);
            emit_agent_status(
                app,
                AgentStatusEmit {
                    session_id: nergal_session_id.unwrap_or(session_id),
                    agent_id: agent_id.as_deref(),
                    model_id: model_id.clone(),
                    model_name: model_name.clone(),
                    session_started_at: *session_started_at,
                    context_used_pct: *context_used_pct,
                    context_window_size: *context_window_size,
                    rate_5h_pct: *rate_5h_pct,
                    rate_5h_resets_at: *rate_5h_resets_at,
                    rate_7d_pct: *rate_7d_pct,
                    rate_7d_resets_at: *rate_7d_resets_at,
                    effort_level: effort_level.clone(),
                },
            );
        }

        HookEvent::AskUser {
            session_id,
            tool_input: _,
            fifo_path: _,
        } => {
            #[derive(Clone, serde::Serialize)]
            struct AskUserPendingPayload {
                session_id: String,
            }
            let _ = app.emit(
                "ask:user-pending",
                AskUserPendingPayload {
                    session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                },
            );
        }

        HookEvent::Notification {
            session_id,
            notification_type,
            message,
        } => {
            tracing::debug!("Notification: type={notification_type:?} message={message:?}");
            #[derive(Clone, serde::Serialize)]
            struct AttentionPendingPayload {
                session_id: String,
                notification_type: Option<String>,
                message: Option<String>,
            }
            let _ = app.emit(
                "attention:pending",
                AttentionPendingPayload {
                    session_id: nergal_session_id.unwrap_or(session_id).to_string(),
                    notification_type: notification_type.clone(),
                    message: message.clone(),
                },
            );
        }
    }
}

/// Inputs for [`emit_agent_status`]. Borrows where it can so the caller
/// (the dispatcher's match arm) doesn't have to clone everything.
pub(crate) struct AgentStatusEmit<'a> {
    pub session_id: &'a str,
    pub agent_id: Option<&'a str>,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub session_started_at: Option<u64>,
    pub context_used_pct: Option<f64>,
    pub context_window_size: Option<u64>,
    pub rate_5h_pct: Option<f64>,
    pub rate_5h_resets_at: Option<u64>,
    pub rate_7d_pct: Option<f64>,
    pub rate_7d_resets_at: Option<u64>,
    pub effort_level: Option<String>,
}

/// Emit `agent:status-update` to the frontend. Single emit path so every
/// adapter (CC statusline script, Pi/Codex Cost translation, future
/// OpenCode SSE) reaches the frontend through the same listener.
pub(crate) fn emit_agent_status(app: &AppHandle, status: AgentStatusEmit<'_>) {
    #[derive(Clone, serde::Serialize)]
    struct AgentStatusPayload {
        session_id: String,
        agent_id: Option<String>,
        model_id: Option<String>,
        model_name: Option<String>,
        session_started_at: Option<u64>,
        context_used_pct: Option<f64>,
        context_window_size: Option<u64>,
        rate_5h_pct: Option<f64>,
        rate_5h_resets_at: Option<u64>,
        rate_7d_pct: Option<f64>,
        rate_7d_resets_at: Option<u64>,
        effort_level: Option<String>,
    }
    let _ = app.emit(
        "agent:status-update",
        AgentStatusPayload {
            session_id: status.session_id.to_string(),
            agent_id: status.agent_id.map(str::to_string),
            model_id: status.model_id,
            model_name: status.model_name,
            session_started_at: status.session_started_at,
            context_used_pct: status.context_used_pct,
            context_window_size: status.context_window_size,
            rate_5h_pct: status.rate_5h_pct,
            rate_5h_resets_at: status.rate_5h_resets_at,
            rate_7d_pct: status.rate_7d_pct,
            rate_7d_resets_at: status.rate_7d_resets_at,
            effort_level: status.effort_level,
        },
    );
}

/// The file a tool's input names, if any. Permissive across the common key
/// spellings so non-CC agents (Pi, Codex) surface too. Shared by the
/// writes-only panel emitter and the MCP `recently_touched_files` capture.
fn file_path_from_tool_input(tool_input: &serde_json::Value) -> Option<&str> {
    tool_input
        .get("file_path")
        .or_else(|| tool_input.get("filePath"))
        .or_else(|| tool_input.get("path"))
        .and_then(|v| v.as_str())
}

/// Extracts file_path from any tool whose input names a file and emits
/// files:modified. Detection is permissive (presence of `file_path` /
/// `filePath` / `path` in the input) rather than a hardcoded tool-name
/// allowlist, so non-CC agents (Pi, Codex) — whose tools are named
/// differently — still surface their file edits.
fn process_file_event(
    app: &AppHandle,
    tool_name: &str,
    tool_input: &serde_json::Value,
    session_id: &str,
) {
    // Skip read-only tools so the panel doesn't flood with files we only
    // looked at. Anything not on this list and that exposes a file_path is
    // assumed to be a write.
    let is_read_only = matches!(tool_name, "Read" | "Glob" | "Grep" | "LS");
    if is_read_only {
        return;
    }

    let Some(path) = file_path_from_tool_input(tool_input) else {
        return;
    };

    #[derive(Clone, serde::Serialize)]
    struct FileModified {
        session_id: String,
        path: String,
        tool: String,
    }

    tracing::debug!("file modified: {path} by {tool_name}");
    let _ = app.emit(
        "files:modified",
        FileModified {
            session_id: session_id.to_string(),
            path: path.to_string(),
            tool: tool_name.to_string(),
        },
    );

    // Notify OpenSpec panel when specs are modified
    if path.contains("/openspec/") {
        let _ = app.emit("openspec:changed", ());
    }
}

/// Routes task-related tool events to SQLite.
fn process_task_event(
    app: &AppHandle,
    tool_name: &str,
    tool_input: &serde_json::Value,
    session_id: &str,
    db: &SharedDb,
) {
    // TodoWrite is a full-replacement operation: the `todos` array is CC's
    // authoritative state for the session. There is no `command` field — the
    // old branch that checked for one silently dropped every TodoWrite event,
    // which is the root cause of BUG-07(a).
    if tool_name == "TodoWrite" {
        let Some(todos) = tool_input.get("todos").and_then(|v| v.as_array()) else {
            return;
        };

        let new_tasks: Vec<crate::tasks::Task> = todos
            .iter()
            .filter_map(|t| {
                let id = t.get("id").and_then(|v| v.as_str())?.to_string();
                // CC uses "content"; fall back to "subject" for non-CC adapters.
                let subject = t
                    .get("content")
                    .or_else(|| t.get("subject"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = t
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let status = match t
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pending")
                {
                    "completed" | "done" => crate::tasks::TaskStatus::Completed,
                    "in_progress" => crate::tasks::TaskStatus::InProgress,
                    _ => crate::tasks::TaskStatus::Pending,
                };
                Some(crate::tasks::Task {
                    id,
                    subject,
                    description,
                    status,
                    active_form: None,
                    blocked_by: vec![],
                })
            })
            .collect();

        let incoming_ids: std::collections::HashSet<&str> =
            new_tasks.iter().map(|t| t.id.as_str()).collect();

        let visible: Vec<crate::tasks::Task>;

        if let Ok(db_guard) = db.lock() {
            // User-tombstoned IDs must not be resurrected even when CC still
            // carries the task in its in-memory list after "Clear completed".
            let tombstoned: std::collections::HashSet<String> = db_guard
                .get_tombstoned_task_ids(session_id)
                .unwrap_or_default()
                .into_iter()
                .collect();

            // Tombstone any visible task CC dropped from its list.
            if let Ok(existing) = db_guard.get_visible_tasks(session_id) {
                for task in &existing {
                    if !incoming_ids.contains(task.id.as_str()) {
                        let _ = db_guard.mark_task_deleted(session_id, &task.id);
                    }
                }
            }

            for task in &new_tasks {
                if !tombstoned.contains(&task.id) {
                    let _ = db_guard.upsert_task(session_id, task);
                }
            }

            visible = new_tasks
                .into_iter()
                .filter(|t| !tombstoned.contains(&t.id))
                .collect();
        } else {
            visible = new_tasks;
        }

        #[derive(Clone, serde::Serialize)]
        struct TasksUpdate {
            session_id: String,
            tasks: Vec<crate::tasks::Task>,
        }
        let _ = app.emit(
            "tasks:update",
            TasksUpdate {
                session_id: session_id.to_string(),
                tasks: visible,
            },
        );
        return;
    }

    let is_task_create;
    let is_task_update;

    if tool_name == "TodoUpdate" || tool_name == "Task" {
        let command = tool_input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        is_task_create = command == "create" || command == "add";
        is_task_update = command == "update" || command == "delete" || command == "complete";
    } else {
        is_task_create = tool_name == "TaskCreate";
        is_task_update = tool_name == "TaskUpdate";
    }

    if !is_task_create && !is_task_update {
        return;
    }

    let mut store = crate::tasks::TaskStore::new();

    if let Ok(db_guard) = db.lock()
        && let Ok(existing) = db_guard.get_visible_tasks(session_id)
    {
        for task in existing {
            store.import_task(task);
        }
    }

    if is_task_create {
        store.apply_create(tool_input);
    } else {
        let matched = store.apply_update(tool_input);
        if !matched {
            // Surface mismatches so a future BUG-21-style repro has a log
            // trail showing which taskId we couldn't find — silent drops
            // hid the issue last quarter.
            let task_id = tool_input
                .get("taskId")
                .and_then(|v| v.as_str())
                .unwrap_or("<missing>");
            let known_ids: Vec<&str> = store.all_tasks().map(|t| t.id.as_str()).collect();
            tracing::warn!(
                "task update missed: tool={tool_name} taskId={task_id} known={known_ids:?}"
            );
        }
    }

    if let Ok(db_guard) = db.lock() {
        for task in store.all_tasks() {
            let _ = db_guard.upsert_task(session_id, task);
        }
    }

    let tasks: Vec<_> = store.visible_tasks().cloned().collect();
    #[derive(Clone, serde::Serialize)]
    struct TasksUpdate {
        session_id: String,
        tasks: Vec<crate::tasks::Task>,
    }
    let _ = app.emit(
        "tasks:update",
        TasksUpdate {
            session_id: session_id.to_string(),
            tasks,
        },
    );
}

// `process_event`'s full dispatch (the match arms above) takes `&AppHandle`,
// `&SharedDb`, `&SharedPlanState`, `&AgentRuntimeState` together and emits
// through the live Tauri event bus; the crate has no `tauri::test` mocking
// feature enabled anywhere (`Cargo.toml`'s `tauri` dependency declares
// `features = []`, and no test in the codebase constructs an `AppHandle`),
// so driving the full side-effecting router is out of reach without adding
// that seam. What IS reachable — and is exactly the event→action *decision*
// surface the router computes before any side effect fires — are the pure
// helpers below: `FrontendHookEvent::from_hook` (the shape emitted for every
// hook event variant on `hook:event`), `resolve_active_plan` (the plan-review
// resolution priority, the highest-blast-radius branch per design D1),
// `session_log_line` (the #2 log line per event), and `file_path_from_tool_input`
// (the file-touch extraction shared by the panel emitter and MCP capture).
// Untested: DB writes, agent_state mutations, and the actual `app.emit` calls
// inside `process_event`'s match arms — those require either a live app or a
// `route_event(event, ctx) -> Vec<Action>` extraction (design D2), which is a
// substantially larger, non-behavior-preserving restructuring of a ~400-line
// effect-per-branch function and was judged out of scope for this pass.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- resolve_active_plan: task 1.2 (active plan found / absent / ambiguous) --

    #[test]
    fn resolve_active_plan_uses_explicit_plan_path_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("explicit.md");
        std::fs::write(&plan_path, "explicit content").unwrap();
        let tool_input = json!({ "plan_path": plan_path.display().to_string() });

        let result = resolve_active_plan(&tool_input, &[]);

        assert_eq!(result, Some((plan_path, "explicit content".to_string())));
    }

    #[test]
    fn resolve_active_plan_none_when_explicit_path_unreadable() {
        let tool_input = json!({ "plan_path": "/nonexistent/dir/plan.md" });

        assert!(resolve_active_plan(&tool_input, &[]).is_none());
    }

    #[test]
    fn resolve_active_plan_finds_correct_dir_by_content_match() {
        // Ambiguous case: two candidate dirs, only the second holds the file whose
        // content matches — proves content-identity search, not "first dir wins".
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        std::fs::write(dir_a.path().join("other.md"), "unrelated plan").unwrap();
        let target = dir_b.path().join("target.md");
        std::fs::write(&target, "the real plan text").unwrap();

        let tool_input = json!({ "plan": "the real plan text" });
        let dirs = vec![dir_a.path().to_path_buf(), dir_b.path().to_path_buf()];
        let result = resolve_active_plan(&tool_input, &dirs);

        assert_eq!(result, Some((target, "the real plan text".to_string())));
    }

    #[test]
    fn resolve_active_plan_falls_back_to_inline_content_when_no_file_matches() {
        // CC delivered the plan text inline but hasn't flushed it to disk yet —
        // still returns the content with a best-effort (possibly default) path.
        let dir = tempfile::tempdir().unwrap();
        let tool_input = json!({ "plan": "not yet flushed to disk" });
        let dirs = vec![dir.path().to_path_buf()];

        let result = resolve_active_plan(&tool_input, &dirs);

        assert_eq!(
            result,
            Some((PathBuf::default(), "not yet flushed to disk".to_string()))
        );
    }

    #[test]
    fn resolve_active_plan_treats_empty_plan_string_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("only.md");
        std::fs::write(&plan_path, "only content").unwrap();
        let tool_input = json!({ "plan": "" });
        let dirs = vec![dir.path().to_path_buf()];

        let result = resolve_active_plan(&tool_input, &dirs);

        assert_eq!(result, Some((plan_path, "only content".to_string())));
    }

    #[test]
    fn resolve_active_plan_legacy_fallback_uses_newest_md_file() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("legacy.md");
        std::fs::write(&plan_path, "legacy content").unwrap();
        let tool_input = json!({});
        let dirs = vec![dir.path().to_path_buf()];

        let result = resolve_active_plan(&tool_input, &dirs);

        assert_eq!(result, Some((plan_path, "legacy content".to_string())));
    }

    #[test]
    fn resolve_active_plan_none_when_nothing_to_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let tool_input = json!({});

        assert!(resolve_active_plan(&tool_input, &[dir.path().to_path_buf()]).is_none());
        assert!(resolve_active_plan(&tool_input, &[]).is_none());
    }

    // -- FrontendHookEvent::from_hook: event→emitted-shape mapping for every variant --

    #[test]
    fn from_hook_maps_every_event_variant_to_its_event_type() {
        let cases: Vec<(HookEvent, &str)> = vec![
            (
                HookEvent::SessionStart {
                    session_id: "s".into(),
                },
                "session_start",
            ),
            (
                HookEvent::SessionEnd {
                    session_id: "s".into(),
                },
                "session_end",
            ),
            (
                HookEvent::PreToolUse {
                    session_id: "s".into(),
                    tool_name: "Read".into(),
                    tool_input: json!({"a": 1}),
                },
                "pre_tool_use",
            ),
            (
                HookEvent::PostToolUse {
                    session_id: "s".into(),
                    tool_name: "Read".into(),
                    tool_input: json!({}),
                    tool_result: None,
                },
                "post_tool_use",
            ),
            (
                HookEvent::Stop {
                    session_id: "s".into(),
                    stop_reason: Some("end_turn".into()),
                    transcript_path: Some("/t.jsonl".into()),
                    background_tasks: vec![],
                    session_crons: vec![],
                    last_assistant_message: None,
                },
                "stop",
            ),
            (
                HookEvent::TaskCompleted {
                    session_id: "s".into(),
                    task_id: None,
                    task_subject: Some("Fix bug".into()),
                },
                "task_completed",
            ),
            (
                HookEvent::UserPromptSubmit {
                    session_id: "s".into(),
                },
                "user_prompt_submit",
            ),
            (
                HookEvent::TaskCreated {
                    session_id: "s".into(),
                    task_id: None,
                    task_subject: Some("New task".into()),
                    tool_input: json!({}),
                },
                "task_created",
            ),
            (
                HookEvent::PlanReview {
                    session_id: "s".into(),
                    tool_name: "ExitPlanMode".into(),
                    tool_input: json!({}),
                    fifo_path: "/f".into(),
                },
                "plan_review",
            ),
            (
                HookEvent::AskUser {
                    session_id: "s".into(),
                    tool_input: json!({}),
                    fifo_path: "/f".into(),
                },
                "ask_user",
            ),
            (
                HookEvent::Notification {
                    session_id: "s".into(),
                    notification_type: Some("permission_prompt".into()),
                    message: None,
                },
                "notification",
            ),
            (
                HookEvent::CwdChanged {
                    session_id: "s".into(),
                    cwd: Some("/cwd".into()),
                },
                "cwd_changed",
            ),
            (
                HookEvent::FileChanged {
                    session_id: "s".into(),
                    file_path: Some("/f.rs".into()),
                    event_type: Some("modified".into()),
                },
                "file_changed",
            ),
            (
                HookEvent::PermissionDenied {
                    session_id: "s".into(),
                    tool_name: Some("Bash".into()),
                    tool_input: json!({}),
                    reason: Some("no".into()),
                },
                "permission_denied",
            ),
            (
                HookEvent::StatusLine {
                    session_id: "s".into(),
                    model_id: None,
                    model_name: None,
                    context_used_pct: None,
                    context_remaining_pct: None,
                    context_window_size: None,
                    rate_5h_pct: None,
                    rate_5h_resets_at: None,
                    rate_7d_pct: None,
                    rate_7d_resets_at: None,
                    duration_ms: None,
                    api_duration_ms: None,
                    lines_added: None,
                    lines_removed: None,
                },
                "statusline",
            ),
            (
                HookEvent::AgentStatus {
                    session_id: "s".into(),
                    agent_id: None,
                    model_id: None,
                    model_name: None,
                    session_started_at: None,
                    context_used_pct: None,
                    context_window_size: None,
                    rate_5h_pct: None,
                    rate_5h_resets_at: None,
                    rate_7d_pct: None,
                    rate_7d_resets_at: None,
                    effort_level: None,
                },
                "agent_status",
            ),
        ];

        assert_eq!(
            cases.len(),
            16,
            "every HookEvent variant must appear exactly once above"
        );

        for (event, expected_type) in cases {
            let fe = FrontendHookEvent::from_hook(&event);
            assert_eq!(fe.event_type, expected_type);
            assert_eq!(fe.session_id, "s");
        }
    }

    #[test]
    fn from_hook_stop_carries_reason_and_transcript_path() {
        let event = HookEvent::Stop {
            session_id: "s".into(),
            stop_reason: Some("end_turn".into()),
            transcript_path: Some("/tmp/t.jsonl".into()),
            background_tasks: vec![],
            session_crons: vec![],
            last_assistant_message: None,
        };

        let fe = FrontendHookEvent::from_hook(&event);

        assert_eq!(fe.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(fe.transcript_path.as_deref(), Some("/tmp/t.jsonl"));
        assert!(fe.tool_name.is_none());
    }

    #[test]
    fn from_hook_permission_denied_reuses_the_stop_reason_slot_for_reason() {
        // PermissionDenied has no dedicated "reason" field on FrontendHookEvent —
        // it's carried in stop_reason. Locking this down so a future refactor of
        // the flat frontend shape doesn't silently drop it.
        let event = HookEvent::PermissionDenied {
            session_id: "s".into(),
            tool_name: Some("Bash".into()),
            tool_input: json!({"command": "rm -rf /"}),
            reason: Some("destructive command".into()),
        };

        let fe = FrontendHookEvent::from_hook(&event);

        assert_eq!(fe.tool_name.as_deref(), Some("Bash"));
        assert_eq!(fe.stop_reason.as_deref(), Some("destructive command"));
        assert_eq!(fe.tool_input, Some(json!({"command": "rm -rf /"})));
    }

    // -- session_log_line: the #2 log line per event --

    #[test]
    fn session_log_line_formats_edit_tools_with_file_path() {
        let event = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Edit".into(),
            tool_input: json!({ "file_path": "src/main.rs" }),
        };

        assert_eq!(
            session_log_line(&event).as_deref(),
            Some("Edit src/main.rs")
        );
    }

    #[test]
    fn session_log_line_formats_read_without_path_as_bare_read() {
        let event = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Read".into(),
            tool_input: json!({}),
        };

        assert_eq!(session_log_line(&event).as_deref(), Some("Read"));
    }

    #[test]
    fn session_log_line_formats_unknown_tools_generically() {
        let event = HookEvent::PreToolUse {
            session_id: "s".into(),
            tool_name: "Bash".into(),
            tool_input: json!({}),
        };

        assert_eq!(session_log_line(&event).as_deref(), Some("Tool Bash"));
    }

    #[test]
    fn session_log_line_skips_empty_stop_reason() {
        let event = HookEvent::Stop {
            session_id: "s".into(),
            stop_reason: Some("".into()),
            transcript_path: None,
            background_tasks: vec![],
            session_crons: vec![],
            last_assistant_message: None,
        };

        assert!(session_log_line(&event).is_none());
    }

    #[test]
    fn session_log_line_formats_permission_denied_with_fallbacks() {
        let event = HookEvent::PermissionDenied {
            session_id: "s".into(),
            tool_name: None,
            tool_input: json!({}),
            reason: None,
        };

        assert_eq!(
            session_log_line(&event).as_deref(),
            Some("Permission denied: ? — —")
        );
    }

    #[test]
    fn session_log_line_none_for_unloggable_events() {
        let event = HookEvent::CwdChanged {
            session_id: "s".into(),
            cwd: Some("/x".into()),
        };

        assert!(session_log_line(&event).is_none());
    }

    // -- file_path_from_tool_input: key-spelling fallback --

    #[test]
    fn file_path_from_tool_input_prefers_file_path_key() {
        let input = json!({ "file_path": "a.rs", "filePath": "b.rs", "path": "c.rs" });

        assert_eq!(file_path_from_tool_input(&input), Some("a.rs"));
    }

    #[test]
    fn file_path_from_tool_input_falls_back_through_key_variants() {
        assert_eq!(
            file_path_from_tool_input(&json!({ "filePath": "b.rs" })),
            Some("b.rs")
        );
        assert_eq!(
            file_path_from_tool_input(&json!({ "path": "c.rs" })),
            Some("c.rs")
        );
        assert_eq!(file_path_from_tool_input(&json!({})), None);
    }
}
