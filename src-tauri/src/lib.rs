mod claude;
mod commands;
pub mod config;
pub mod hooks;
mod pty;
pub mod setup;
mod tasks;

use std::sync::{Arc, Mutex};

use claude::plan::{PlanManager, PlanWatcher, SharedPlanManager};
use claude::transcript::TranscriptWatcher;
use commands::SharedTaskStore;
use config::Config;
use hooks::server::start_hook_server;
use pty::PtyManager;
use tasks::TaskStore;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::load();

    let plan_manager: SharedPlanManager =
        Arc::new(Mutex::new(PlanManager::new(config.plans_directory.clone())));
    let task_store: SharedTaskStore = Arc::new(Mutex::new(TaskStore::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(PtyManager::new())
        .manage(plan_manager.clone())
        .manage(task_store.clone())
        .invoke_handler(tauri::generate_handler![
            // PTY commands
            pty::pty_create,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
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
            // Setup command
            commands::setup_hooks,
            // Cost command
            commands::get_cost,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let socket_path = config.hook_socket_path.clone();
            let plans_dir = config.plans_directory.clone();
            let transcripts_dir = config.transcripts_directory.clone();

            // Start hook server in background with access to plan/task state
            let hook_app = app_handle.clone();
            let hook_plan = plan_manager.clone();
            let hook_tasks = task_store.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    start_hook_server(&socket_path, hook_app, hook_plan, hook_tasks).await
                {
                    tracing::error!("hook server error: {e}");
                }
            });

            // Start plan watcher (needs the directory to exist)
            if plans_dir.exists() {
                match PlanWatcher::new(&plans_dir, app_handle.clone()) {
                    Ok(watcher) => {
                        // Leak the watcher so it stays alive for the app lifetime
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

            // Start transcript watcher
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
