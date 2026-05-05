use std::path::Path;

use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;

use super::events::HookEvent;
use crate::agents::AgentId;
use crate::agents::state::AgentRuntimeState;
use crate::db::SharedDb;
use crate::plan_state::SharedPlanState;

/// Flat event structure the frontend expects.
#[derive(Clone, serde::Serialize)]
struct FrontendHookEvent {
    session_id: String,
    cluihud_session_id: Option<String>,
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
            };
        Self {
            session_id,
            cluihud_session_id: None,
            event_type: event_type.into(),
            tool_name,
            tool_input,
            stop_reason,
            transcript_path,
        }
    }
}

/// Starts the Unix socket server that receives hook events from Claude CLI.
pub async fn start_hook_server(
    socket_path: &Path,
    app: AppHandle,
    db: SharedDb,
    plan_state: SharedPlanState,
    agent_state: AgentRuntimeState,
) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!("hook server listening on {}", socket_path.display());

    loop {
        let (stream, _addr) = listener.accept().await?;
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

                let cluihud_sid = parsed.as_ref().and_then(|v| {
                    v.get("cluihud_session_id")
                        .and_then(|s| s.as_str())
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
                            cluihud_sid.as_deref(),
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
/// the event's own session_id as the cluihud_session_id (adapters embed it
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

fn process_event(
    app: &AppHandle,
    event: &HookEvent,
    db: &SharedDb,
    plan_state: &SharedPlanState,
    agent_state: &AgentRuntimeState,
    cluihud_session_id: Option<&str>,
) {
    if cluihud_session_id.is_none() {
        tracing::debug!("ignoring hook event without cluihud_session_id");
        return;
    }

    // Resolve the owning adapter for this session (cache → DB-fallback path
    // lands once the DB schema carries agent_id; until then unknown sessions
    // are tagged claude-code defensively to preserve current behavior). Drop
    // events whose session is fully unknown — they are orphans.
    if let Some(csid) = cluihud_session_id
        && agent_state.resolve(csid).is_none()
    {
        tracing::warn!(
            cluihud_session_id = %csid,
            event_type = ?std::any::type_name_of_val(event),
            "hook event for session not in agent cache; assuming claude-code (foundation transitional)"
        );
        agent_state.register_session(csid, AgentId::claude_code());
    }

    let mut frontend_event = FrontendHookEvent::from_hook(event);
    frontend_event.cluihud_session_id = cluihud_session_id.map(String::from);
    let _ = app.emit("hook:event", &frontend_event);

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
        }

        HookEvent::SessionEnd { session_id } => {
            let _ = app.emit("session:end", session_id);
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
                let runtime = state.get_or_create(session_id);
                let plan_path = tool_input
                    .get("plan_path")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        // Fallback: find latest plan in project-local .claude/plans/
                        if let Some(csid) = cluihud_session_id
                            && let Ok(db_guard) = db.lock()
                            && let Ok(Some(session)) = db_guard.find_session(csid)
                        {
                            let cwd = session.worktree_path.or_else(|| {
                                db_guard
                                    .workspace_repo_path(&session.workspace_id)
                                    .ok()
                                    .flatten()
                            });
                            if let Some(cwd) = cwd {
                                let local_plans = cwd.join(".claude").join("plans");
                                if local_plans.exists() {
                                    let local_mgr =
                                        crate::agents::claude_code::plan::PlanManager::new(
                                            local_plans,
                                        );
                                    return local_mgr.find_latest_plan().ok().flatten();
                                }
                            }
                        }
                        // Last fallback: global plans dir
                        runtime.find_latest_plan().ok().flatten()
                    });

                if let Some(path) = plan_path
                    && let Ok(()) = runtime.load_plan(&path)
                {
                    #[derive(Clone, serde::Serialize)]
                    struct PlanReady {
                        path: String,
                        content: String,
                        session_id: String,
                    }
                    if let Some(content) = runtime.current_content() {
                        let _ = app.emit(
                            "plan:ready",
                            PlanReady {
                                path: path.display().to_string(),
                                content: content.to_string(),
                                session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
                            },
                        );
                    }
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
            let csid = cluihud_session_id.unwrap_or(session_id);
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

            // Store cost in DB
            if let Ok(db_guard) = db.lock() {
                let _ = db_guard.upsert_cost(session_id, &cost);
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
                    session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
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
            let csid = cluihud_session_id.unwrap_or(session_id);
            process_task_event(app, "TaskCreate", tool_input, csid, db);
        }

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
            if let Some(csid) = cluihud_session_id {
                agent_state
                    .claude_code
                    .register_pending_plan_fifo(csid, std::path::PathBuf::from(fifo_path));
            }

            if let Ok(mut state) = plan_state.lock() {
                let runtime = state.get_or_create(session_id);
                let plan_path = tool_input
                    .get("plan_path")
                    .and_then(|v| v.as_str())
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        if let Some(csid) = cluihud_session_id
                            && let Ok(db_guard) = db.lock()
                            && let Ok(Some(session)) = db_guard.find_session(csid)
                        {
                            let cwd = session.worktree_path.or_else(|| {
                                db_guard
                                    .workspace_repo_path(&session.workspace_id)
                                    .ok()
                                    .flatten()
                            });
                            if let Some(cwd) = cwd {
                                let local_plans = cwd.join(".claude").join("plans");
                                if local_plans.exists() {
                                    let local_mgr =
                                        crate::agents::claude_code::plan::PlanManager::new(
                                            local_plans,
                                        );
                                    return local_mgr.find_latest_plan().ok().flatten();
                                }
                            }
                        }
                        runtime.find_latest_plan().ok().flatten()
                    });

                if let Some(path) = plan_path
                    && let Ok(()) = runtime.load_plan(&path)
                {
                    #[derive(Clone, serde::Serialize)]
                    struct PlanReady {
                        path: String,
                        content: String,
                        session_id: String,
                        decision_path: String,
                    }
                    if let Some(content) = runtime.current_content() {
                        let _ = app.emit(
                            "plan:ready",
                            PlanReady {
                                path: path.display().to_string(),
                                content: content.to_string(),
                                session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
                                decision_path: fifo_path.clone(),
                            },
                        );
                    }
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
                        session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
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
                        session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
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
                    session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
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
            context_remaining_pct,
            context_window_size,
            rate_5h_pct,
            rate_5h_resets_at,
            rate_7d_pct,
            rate_7d_resets_at,
            duration_ms,
            api_duration_ms,
            lines_added,
            lines_removed,
        } => {
            #[derive(Clone, serde::Serialize)]
            struct StatusLinePayload {
                session_id: String,
                model_id: Option<String>,
                model_name: Option<String>,
                context_used_pct: Option<f64>,
                context_remaining_pct: Option<f64>,
                context_window_size: Option<u64>,
                rate_5h_pct: Option<f64>,
                rate_5h_resets_at: Option<u64>,
                rate_7d_pct: Option<f64>,
                rate_7d_resets_at: Option<u64>,
                duration_ms: Option<u64>,
                api_duration_ms: Option<u64>,
                lines_added: Option<u64>,
                lines_removed: Option<u64>,
            }
            let _ = app.emit(
                "statusline:update",
                StatusLinePayload {
                    session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
                    model_id: model_id.clone(),
                    model_name: model_name.clone(),
                    context_used_pct: *context_used_pct,
                    context_remaining_pct: *context_remaining_pct,
                    context_window_size: *context_window_size,
                    rate_5h_pct: *rate_5h_pct,
                    rate_5h_resets_at: *rate_5h_resets_at,
                    rate_7d_pct: *rate_7d_pct,
                    rate_7d_resets_at: *rate_7d_resets_at,
                    duration_ms: *duration_ms,
                    api_duration_ms: *api_duration_ms,
                    lines_added: *lines_added,
                    lines_removed: *lines_removed,
                },
            );
        }

        HookEvent::AskUser {
            session_id,
            tool_input,
            fifo_path,
        } => {
            tracing::debug!("AskUser: fifo_path={fifo_path}");

            if let Some(csid) = cluihud_session_id {
                agent_state
                    .claude_code
                    .register_pending_ask_fifo(csid, std::path::PathBuf::from(fifo_path));
            }

            #[derive(Clone, serde::Serialize)]
            struct AskUserQuestion {
                question: String,
                header: String,
                options: Vec<String>,
                multi_select: bool,
            }

            #[derive(Clone, serde::Serialize)]
            struct AskUserPayload {
                session_id: String,
                questions: Vec<AskUserQuestion>,
                decision_path: String,
            }

            let mut questions = Vec::new();
            if let Some(arr) = tool_input.get("questions").and_then(|v| v.as_array()) {
                for q in arr {
                    let question = q
                        .get("question")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let header = q
                        .get("header")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let multi_select = q
                        .get("multiSelect")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let options = q
                        .get("options")
                        .and_then(|v| v.as_array())
                        .map(|opts| {
                            opts.iter()
                                .filter_map(|o| {
                                    o.get("label").and_then(|l| l.as_str()).map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    questions.push(AskUserQuestion {
                        question,
                        header,
                        options,
                        multi_select,
                    });
                }
            }

            let _ = app.emit(
                "ask:user",
                AskUserPayload {
                    session_id: cluihud_session_id.unwrap_or(session_id).to_string(),
                    questions,
                    decision_path: fifo_path.clone(),
                },
            );
        }
    }
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

    let file_path = tool_input
        .get("file_path")
        .or_else(|| tool_input.get("filePath"))
        .or_else(|| tool_input.get("path"))
        .and_then(|v| v.as_str());

    let Some(path) = file_path else {
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
    let is_task_create;
    let is_task_update;

    if tool_name == "TodoWrite" || tool_name == "TodoUpdate" || tool_name == "Task" {
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

    // Use in-memory TaskStore for event processing, then persist to DB
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
        store.apply_update(tool_input);
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
