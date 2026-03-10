use std::path::Path;

use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;

use super::events::HookEvent;
use crate::claude::plan::SharedPlanManager;
use crate::commands::SharedTaskStore;

/// Flat event structure the frontend expects.
#[derive(Clone, serde::Serialize)]
struct FrontendHookEvent {
    session_id: String,
    event_type: String,
    tool_name: Option<String>,
    tool_input: Option<serde_json::Value>,
    stop_reason: Option<String>,
    transcript_path: Option<String>,
}

impl From<&HookEvent> for FrontendHookEvent {
    fn from(event: &HookEvent) -> Self {
        match event {
            HookEvent::SessionStart { session_id } => Self {
                session_id: session_id.clone(),
                event_type: "session_start".into(),
                tool_name: None,
                tool_input: None,
                stop_reason: None,
                transcript_path: None,
            },
            HookEvent::SessionEnd { session_id } => Self {
                session_id: session_id.clone(),
                event_type: "session_end".into(),
                tool_name: None,
                tool_input: None,
                stop_reason: None,
                transcript_path: None,
            },
            HookEvent::PreToolUse {
                session_id,
                tool_name,
                tool_input,
            } => Self {
                session_id: session_id.clone(),
                event_type: "pre_tool_use".into(),
                tool_name: Some(tool_name.clone()),
                tool_input: Some(tool_input.clone()),
                stop_reason: None,
                transcript_path: None,
            },
            HookEvent::PostToolUse {
                session_id,
                tool_name,
                tool_input,
                ..
            } => Self {
                session_id: session_id.clone(),
                event_type: "post_tool_use".into(),
                tool_name: Some(tool_name.clone()),
                tool_input: Some(tool_input.clone()),
                stop_reason: None,
                transcript_path: None,
            },
            HookEvent::Stop {
                session_id,
                stop_reason,
                transcript_path,
            } => Self {
                session_id: session_id.clone(),
                event_type: "stop".into(),
                tool_name: None,
                tool_input: None,
                stop_reason: stop_reason.clone(),
                transcript_path: transcript_path.clone(),
            },
            HookEvent::TaskCompleted {
                session_id,
                task_subject,
                ..
            } => Self {
                session_id: session_id.clone(),
                event_type: "task_completed".into(),
                tool_name: task_subject.clone(),
                tool_input: None,
                stop_reason: None,
                transcript_path: None,
            },
            HookEvent::UserPromptSubmit { session_id } => Self {
                session_id: session_id.clone(),
                event_type: "user_prompt_submit".into(),
                tool_name: None,
                tool_input: None,
                stop_reason: None,
                transcript_path: None,
            },
        }
    }
}

/// Starts the Unix socket server that receives hook events from Claude CLI.
pub async fn start_hook_server(
    socket_path: &Path,
    app: AppHandle,
    plan_manager: SharedPlanManager,
    task_store: SharedTaskStore,
) -> Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!("hook server listening on {}", socket_path.display());

    loop {
        let (stream, _addr) = listener.accept().await?;
        let app = app.clone();
        let plan_mgr = plan_manager.clone();
        let tasks = task_store.clone();

        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stream);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<HookEvent>(&line) {
                    Ok(event) => {
                        process_event(&app, &event, &plan_mgr, &tasks);
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
    plan_manager: &SharedPlanManager,
    task_store: &SharedTaskStore,
) {
    // Emit flat event for the frontend activity log / status bar
    let frontend_event = FrontendHookEvent::from(event);
    let _ = app.emit("hook:event", &frontend_event);

    match event {
        HookEvent::SessionStart { session_id } => {
            #[derive(Clone, serde::Serialize)]
            struct SessionStartPayload {
                id: String,
                active: bool,
            }
            let _ = app.emit(
                "session:start",
                SessionStartPayload {
                    id: session_id.clone(),
                    active: true,
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

            // ExitPlanMode → load plan and emit plan:ready
            if tool_name == "ExitPlanMode" {
                if let Ok(mut mgr) = plan_manager.lock() {
                    // Try to find the plan path from tool_input or find latest
                    let plan_path = tool_input
                        .get("plan_path")
                        .and_then(|v| v.as_str())
                        .map(std::path::PathBuf::from)
                        .or_else(|| mgr.find_latest_plan().ok().flatten());

                    if let Some(path) = plan_path {
                        if let Ok(()) = mgr.load_plan(&path) {
                            #[derive(Clone, serde::Serialize)]
                            struct PlanReady {
                                path: String,
                                content: String,
                            }
                            if let Some(content) = mgr.current_content() {
                                let _ = app.emit(
                                    "plan:ready",
                                    PlanReady {
                                        path: path.display().to_string(),
                                        content: content.to_string(),
                                    },
                                );
                            }
                        }
                    }
                }
            }

            process_task_event(app, tool_name, tool_input, session_id, task_store);
        }

        HookEvent::PostToolUse {
            tool_name,
            tool_input,
            session_id,
            ..
        } => {
            tracing::debug!("PostToolUse: tool_name={tool_name}");
            process_task_event(app, tool_name, tool_input, session_id, task_store);
        }

        HookEvent::Stop {
            transcript_path, ..
        } => {
            // Parse cost from transcript and emit
            if let Some(path) = transcript_path {
                let cost = crate::claude::cost::parse_cost_from_transcript(&std::path::PathBuf::from(path));
                let _ = app.emit("cost:update", &cost);
            }
        }

        _ => {}
    }
}

/// Routes task-related tool events to the TaskStore.
/// Handles both `TaskCreate`/`TaskUpdate` and `TodoWrite` (with `command` field) tool names.
fn process_task_event(
    app: &AppHandle,
    tool_name: &str,
    tool_input: &serde_json::Value,
    session_id: &str,
    task_store: &SharedTaskStore,
) {
    let is_task_create;
    let is_task_update;

    if tool_name == "TodoWrite" || tool_name == "TodoUpdate" || tool_name == "Task" {
        // Claude Code uses TodoWrite with a `command` field
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

    if let Ok(mut store) = task_store.lock() {
        if is_task_create {
            store.apply_create(tool_input);
        } else {
            store.apply_update(tool_input);
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
}
