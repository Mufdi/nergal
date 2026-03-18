use std::path::Path;

use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;

use super::events::HookEvent;
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
        let (session_id, event_type, tool_name, tool_input, stop_reason, transcript_path) = match event {
            HookEvent::SessionStart { session_id } => (session_id.clone(), "session_start", None, None, None, None),
            HookEvent::SessionEnd { session_id } => (session_id.clone(), "session_end", None, None, None, None),
            HookEvent::PreToolUse { session_id, tool_name, tool_input } => (session_id.clone(), "pre_tool_use", Some(tool_name.clone()), Some(tool_input.clone()), None, None),
            HookEvent::PostToolUse { session_id, tool_name, tool_input, .. } => (session_id.clone(), "post_tool_use", Some(tool_name.clone()), Some(tool_input.clone()), None, None),
            HookEvent::Stop { session_id, stop_reason, transcript_path } => (session_id.clone(), "stop", None, None, stop_reason.clone(), transcript_path.clone()),
            HookEvent::TaskCompleted { session_id, task_subject, .. } => (session_id.clone(), "task_completed", task_subject.clone(), None, None, None),
            HookEvent::UserPromptSubmit { session_id } => (session_id.clone(), "user_prompt_submit", None, None, None, None),
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

        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stream);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let cluihud_sid = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|v| v.get("cluihud_session_id").and_then(|s| s.as_str()).map(String::from));

                match serde_json::from_str::<HookEvent>(&line) {
                    Ok(event) => {
                        process_event(&app, &event, &db, &plan_state, cluihud_sid.as_deref());
                    }
                    Err(e) => {
                        tracing::warn!("failed to parse hook event: {e}");
                    }
                }
            }
        });
    }
}

fn process_event(
    app: &AppHandle,
    event: &HookEvent,
    db: &SharedDb,
    plan_state: &SharedPlanState,
    cluihud_session_id: Option<&str>,
) {
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
                    .or_else(|| runtime.find_latest_plan().ok().flatten());

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
                                session_id: session_id.clone(),
                            },
                        );
                    }
                }
            }

            process_task_event(app, tool_name, tool_input, session_id, db);
        }

        HookEvent::PostToolUse {
            tool_name,
            tool_input,
            session_id,
            ..
        } => {
            tracing::debug!("PostToolUse: tool_name={tool_name}");
            process_task_event(app, tool_name, tool_input, session_id, db);
            process_file_event(app, tool_name, tool_input, session_id);
        }

        HookEvent::Stop {
            session_id,
            transcript_path: Some(path),
            ..
        } => {
            let cost =
                crate::claude::cost::parse_cost_from_transcript(&std::path::PathBuf::from(path));

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
                    session_id: session_id.clone(),
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

        HookEvent::UserPromptSubmit { .. } => {}
    }
}

/// Extracts file_path from Write/Edit/MultiEdit tools and emits files:modified.
fn process_file_event(
    app: &AppHandle,
    tool_name: &str,
    tool_input: &serde_json::Value,
    session_id: &str,
) {
    let is_file_tool = matches!(
        tool_name,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "Create"
    );
    if !is_file_tool {
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

    tracing::info!("task event: tool={tool_name} create={is_task_create} update={is_task_update}");

    // Use in-memory TaskStore for event processing, then persist to DB
    let mut store = crate::tasks::TaskStore::new();

    // Load existing tasks from DB into the store
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

    // Persist all tasks back to DB
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
