mod claude;
mod commands;
pub mod config;
mod db;
pub mod hooks;
mod models;
mod plan_state;
mod pty;
pub mod setup;
mod tasks;
mod terminal;
mod worktree;

use claude::openspec::OpenSpecWatcher;
use claude::plan::PlanWatcher;
use claude::transcript::TranscriptWatcher;
use config::Config;
use db::Database;
use hooks::server::start_hook_server;
use plan_state::PlanStateManager;
use pty::PtyManager;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::load();

    let db: db::SharedDb = std::sync::Arc::new(std::sync::Mutex::new(
        Database::open().expect("failed to open database"),
    ));

    let plan_state: plan_state::SharedPlanState = std::sync::Arc::new(std::sync::Mutex::new(
        PlanStateManager::new(config.plans_directory.clone()),
    ));

    // Startup reconciliation: clean up sessions with missing worktree paths
    if let Ok(db_guard) = db.lock() {
        reconcile_worktrees(&db_guard);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(PtyManager::new(config.terminal_kitty_keyboard))
        .manage(db.clone())
        .manage(plan_state.clone())
        .invoke_handler(tauri::generate_handler![
            // PTY commands
            pty::pty_create,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::start_claude_session,
            pty::kill_session_pty,
            pty::terminal_input,
            pty::terminal_get_full_grid,
            pty::resize_session_terminal,
            // Config commands
            commands::get_config,
            commands::save_config,
            // Task commands
            commands::get_tasks,
            // Plan commands
            commands::get_plan,
            commands::save_plan,
            commands::diff_plan,
            commands::approve_plan,
            commands::reject_plan,
            commands::submit_plan_decision,
            commands::submit_ask_answer,
            commands::list_plans,
            commands::list_session_plans,
            commands::load_plan,
            // Annotation commands
            commands::save_annotation,
            commands::get_annotations,
            commands::delete_annotation,
            commands::clear_annotations,
            commands::set_pending_annotations,
            // Buddy command
            commands::get_buddy,
            // Setup command
            commands::send_notification,
            commands::setup_hooks,
            // Cost command
            commands::get_cost,
            // Workspace commands
            commands::create_workspace,
            commands::get_workspaces,
            commands::delete_workspace,
            // Session commands
            commands::create_session,
            commands::delete_session,
            commands::rename_session,
            commands::list_branches,
            commands::merge_session,
            commands::get_transcript,
            commands::check_session_has_commits,
            commands::get_session_git_info,
            commands::get_file_diff,
            commands::list_openspec_changes,
            commands::read_openspec_artifact,
            commands::write_openspec_artifact,
            commands::detect_editors,
            commands::open_in_editor,
            commands::get_git_status,
            commands::git_stage_file,
            commands::git_unstage_file,
            commands::git_stage_all,
            commands::git_unstage_all,
            commands::git_commit,
            commands::get_recent_commits,
            commands::get_pr_status,
            commands::create_pr,
            commands::get_session_changed_files,
            commands::get_commit_files,
            commands::list_directory,
            commands::read_file_content,
            commands::write_file_content,
            pty::write_to_session_pty,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let socket_path = config.hook_socket_path.clone();
            let plans_dir = config.plans_directory.clone();
            let transcripts_dir = config.transcripts_directory.clone();

            let hook_app = app_handle.clone();
            let hook_db = db.clone();
            let hook_plan = plan_state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    start_hook_server(&socket_path, hook_app, hook_db, hook_plan).await
                {
                    tracing::error!("hook server error: {e}");
                }
            });

            if plans_dir.exists() {
                match PlanWatcher::new(&plans_dir, app_handle.clone()) {
                    Ok(watcher) => {
                        Box::leak(Box::new(watcher));
                        tracing::info!("plan watcher started on {}", plans_dir.display());
                    }
                    Err(e) => tracing::warn!("failed to start plan watcher: {e}"),
                }
            } else {
                tracing::info!(
                    "plans directory does not exist yet, skipping watcher: {}",
                    plans_dir.display()
                );
            }

            if transcripts_dir.exists() {
                match TranscriptWatcher::new(&transcripts_dir, app_handle.clone()) {
                    Ok(watcher) => {
                        Box::leak(Box::new(watcher));
                        tracing::info!(
                            "transcript watcher started on {}",
                            transcripts_dir.display()
                        );
                    }
                    Err(e) => tracing::warn!("failed to start transcript watcher: {e}"),
                }
            } else {
                tracing::info!(
                    "transcripts directory does not exist yet, skipping watcher: {}",
                    transcripts_dir.display()
                );
            }

            // OpenSpec watcher — watches the project's openspec/ dir
            // During dev, cwd may be src-tauri/, so also check parent
            let cwd = std::env::current_dir().unwrap_or_default();
            let openspec_dir = if cwd.join("openspec").exists() {
                cwd.join("openspec")
            } else if let Some(parent) = cwd.parent() {
                parent.join("openspec")
            } else {
                cwd.join("openspec")
            };
            if openspec_dir.exists() {
                match OpenSpecWatcher::new(&openspec_dir, app_handle.clone()) {
                    Ok(watcher) => {
                        Box::leak(Box::new(watcher));
                        tracing::info!(
                            "openspec watcher started on {}",
                            openspec_dir.display()
                        );
                    }
                    Err(e) => tracing::warn!("failed to start openspec watcher: {e}"),
                }
            } else {
                tracing::info!(
                    "openspec directory does not exist yet, skipping watcher: {}",
                    openspec_dir.display()
                );
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn reconcile_worktrees(db: &Database) {
    let sessions = match db.sessions_with_worktrees() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to query sessions for reconciliation: {e}");
            return;
        }
    };

    for (session_id, wt_path) in sessions {
        if !wt_path.exists() {
            tracing::warn!(
                "worktree missing for session {session_id}: {}, resetting",
                wt_path.display()
            );
            let _ = db.clear_session_worktree(&session_id);
        }
    }
}
