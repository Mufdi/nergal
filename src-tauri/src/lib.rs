pub mod agents;
mod browser;
mod commands;
pub mod config;
mod db;
pub mod hooks;
mod models;
mod openspec;
mod plan_state;
mod pty;
pub mod scratchpad;
pub mod setup;
mod tasks;
mod terminal;
mod updater;
mod worktree;

use agents::claude_code::plan::{PlanWatcher, SharedPlanWatcher};
use agents::claude_code::transcript::TranscriptWatcher;
use agents::state::AgentRuntimeState;
use config::Config;
use db::Database;
use hooks::server::start_hook_server;
use openspec::OpenSpecWatcher;
use plan_state::PlanStateManager;
use pty::PtyManager;
use scratchpad::commands::ScratchpadState;
use std::path::PathBuf;
use tauri::Manager;

/// When launched under systemd (gnome-shell Activities, .desktop entries on
/// kernel 6.8.0-117+), stdout/stderr are a journald pipe (`JOURNAL_STREAM`
/// env set). `tracing_subscriber` INFO output + WebKitGTK's stderr noise
/// saturates the pipe; the resulting `write()` backpressure stalls the main
/// thread before the frontend reaches `getCurrentWindow().show()`, leaving
/// a 10x10 invisible ghost window. Mitigation: divert fd 1/2 to a regular
/// file. No-op when launched from a terminal (no `JOURNAL_STREAM`).
#[cfg(target_os = "linux")]
fn redirect_journald_stdio_to_logfile() {
    if std::env::var_os("JOURNAL_STREAM").is_none() {
        return;
    }
    let log_path = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("cluihud")
        .join("cluihud.log");
    if let Some(parent) = log_path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("cluihud: log dir {} unavailable: {e}", parent.display());
        return;
    }
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cluihud: cannot open log {}: {e}", log_path.display());
            return;
        }
    };
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    // SAFETY: dup2 with a valid open fd; called once at process start before
    // any threads or tokio runtime exist, so no concurrent fd access.
    unsafe {
        if libc::dup2(fd, libc::STDOUT_FILENO) < 0 {
            eprintln!(
                "cluihud: dup2 stdout failed: {}",
                std::io::Error::last_os_error()
            );
        }
        if libc::dup2(fd, libc::STDERR_FILENO) < 0 {
            eprintln!(
                "cluihud: dup2 stderr failed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    // `file` drops here, closing the original fd. The duplicated 1/2 keep
    // the underlying open file description alive.
}

#[cfg(not(target_os = "linux"))]
fn redirect_journald_stdio_to_logfile() {}

/// File written to `~/.cluihud-active` while the app is running, so the
/// `cluihud-conditional.sh` hook wrapper can detect whether forwarding
/// hook events to `cluihud hook ...` is worth doing. Removed on Drop.
struct SentinelGuard {
    path: PathBuf,
}

impl SentinelGuard {
    fn new() -> Self {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".cluihud-active");
        if let Err(e) = std::fs::write(&path, std::process::id().to_string()) {
            tracing::warn!("failed to write sentinel {}: {e}", path.display());
        }
        Self { path }
    }
}

impl Drop for SentinelGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Linux desktop launchers hand us a minimal PATH (no nvm, no `~/.opencode/bin`),
/// so `which::which("pi"|"opencode")` fails, the registry reports only Claude
/// Code as installed, and the agent picker silently auto-resolves to CC.
fn augment_path_from_user_shell() {
    #[cfg(target_os = "linux")]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        // `-l -i -c` is required: many users only export PATH from interactive
        // rc files (`.zshrc`, `.bashrc`), not from login profiles.
        let output = match std::process::Command::new(&shell)
            .args(["-l", "-i", "-c", "printf %s \"$PATH\""])
            .output()
        {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                tracing::debug!(
                    "shell PATH probe exited with {:?}, skipping augment",
                    o.status.code()
                );
                return;
            }
            Err(e) => {
                tracing::debug!("shell PATH probe failed ({e}), skipping augment");
                return;
            }
        };
        let shell_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if shell_path.is_empty() {
            return;
        }

        let current = std::env::var("PATH").unwrap_or_default();
        let current_set: std::collections::HashSet<&str> = current.split(':').collect();
        let mut additions: Vec<&str> = Vec::new();
        for entry in shell_path.split(':') {
            if !entry.is_empty() && !current_set.contains(entry) {
                additions.push(entry);
            }
        }
        if additions.is_empty() {
            return;
        }
        let merged = if current.is_empty() {
            additions.join(":")
        } else {
            format!("{}:{}", additions.join(":"), current)
        };
        // SAFETY: called once, single-threaded, before any tokio runtime or
        // child spawn — no other thread can be reading PATH concurrently.
        unsafe {
            std::env::set_var("PATH", &merged);
        }
        tracing::info!(
            "PATH augmented with {} entries from {}",
            additions.len(),
            shell
        );
    }
}

pub fn run() {
    redirect_journald_stdio_to_logfile();
    let _sentinel = SentinelGuard::new();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    augment_path_from_user_shell();

    let config = Config::load();

    let db: db::SharedDb = std::sync::Arc::new(std::sync::Mutex::new(
        Database::open().expect("failed to open database"),
    ));

    let plan_state: plan_state::SharedPlanState = std::sync::Arc::new(std::sync::Mutex::new(
        PlanStateManager::new(dirs::home_dir().unwrap_or_default().join(".claude/plans")),
    ));

    let agent_state = AgentRuntimeState::bootstrap()
        .expect("failed to bootstrap agent runtime state with default registrations");

    // Take the EventSink receiver from the bootstrap'd runtime state — the
    // consumer task is spawned from the Tauri setup callback below where
    // AppHandle is available.
    let agent_event_rx = agent_state
        .take_event_receiver()
        .expect("event receiver must be available exactly once at startup");

    let scratchpad_root = config
        .scratchpad_path
        .clone()
        .unwrap_or_else(scratchpad::default_scratchpad_dir);
    if let Err(e) = scratchpad::ensure_dir(&scratchpad_root) {
        tracing::warn!("scratchpad ensure_dir failed: {e}");
    }
    if let Err(e) = scratchpad::cleanup_orphan_tmps(&scratchpad_root) {
        tracing::warn!("scratchpad cleanup_orphan_tmps failed: {e}");
    }
    match scratchpad::purge::purge_trash(&scratchpad_root) {
        Ok(0) => {}
        Ok(n) => tracing::info!("scratchpad purge: removed {n} expired notes"),
        Err(e) => tracing::warn!("scratchpad purge failed: {e}"),
    }

    // Startup reconciliation: clean up sessions with missing worktree paths
    if let Ok(db_guard) = db.lock() {
        reconcile_worktrees(&db_guard);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(PtyManager::new(config.terminal_kitty_keyboard))
        .manage(db.clone())
        .manage(plan_state.clone())
        .manage(agent_state.clone())
        .manage(ScratchpadState::new(scratchpad_root.clone()))
        .invoke_handler(tauri::generate_handler![
            // PTY commands
            pty::start_claude_session,
            pty::kill_session_pty,
            pty::write_to_session_pty,
            pty::resize_session_terminal,
            pty::terminal_input,
            pty::terminal_get_full_grid,
            pty::terminal_scroll,
            pty::terminal_scroll_to_bottom,
            pty::terminal_mouse_button,
            pty::terminal_paste,
            pty::terminal_clipboard_write,
            // Config commands
            commands::get_config,
            commands::save_config,
            commands::validate_path,
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
            commands::list_session_plans,
            commands::get_session_plan_capability,
            commands::load_plan,
            // Annotation commands
            commands::save_annotation,
            commands::get_annotations,
            commands::delete_annotation,
            commands::clear_annotations,
            commands::set_pending_annotations,
            commands::save_spec_annotation,
            commands::get_spec_annotations,
            commands::delete_spec_annotation,
            commands::clear_spec_annotations,
            commands::count_spec_annotations_by_prefix,
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
            commands::list_prs,
            commands::get_pr_diff,
            commands::get_pr_checks,
            commands::gh_pr_merge,
            commands::merge_session,
            commands::cleanup_merged_session,
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
            commands::git_stash_list,
            commands::git_stash_create,
            commands::git_stash_apply,
            commands::git_stash_pop,
            commands::git_stash_drop,
            commands::git_stash_show,
            commands::git_stash_branch,
            commands::get_recent_commits,
            commands::get_pr_status,
            commands::create_pr,
            commands::git_push,
            commands::git_ship,
            commands::get_pr_preview_data,
            commands::poll_pr_checks,
            commands::gh_available,
            commands::get_conflicted_files,
            commands::get_file_conflict_versions,
            commands::save_conflict_resolution,
            commands::enqueue_conflict_context,
            commands::build_conflict_prompt,
            commands::pull_target_into_session,
            commands::complete_pending_merge,
            commands::has_pending_merge,
            commands::enable_pr_auto_merge,
            commands::get_session_changed_files,
            commands::get_commit_files,
            commands::list_directory,
            commands::search_files,
            commands::read_file_content,
            commands::write_file_content,
            commands::list_available_agents,
            commands::resolve_default_agent,
            commands::apply_theme_to_agents,
            pty::write_to_session_pty,
            // Scratchpad commands
            scratchpad::commands::scratchpad_get_path,
            scratchpad::commands::scratchpad_default_path,
            scratchpad::commands::scratchpad_set_path,
            scratchpad::commands::scratchpad_list_tabs,
            scratchpad::commands::scratchpad_read_tab,
            scratchpad::commands::scratchpad_write_tab,
            scratchpad::commands::scratchpad_create_tab,
            scratchpad::commands::scratchpad_close_tab,
            scratchpad::commands::scratchpad_cleanup_tmps,
            scratchpad::commands::scratchpad_restore_tab,
            scratchpad::commands::scratchpad_get_geometry,
            scratchpad::commands::scratchpad_set_geometry,
            scratchpad::commands::scratchpad_reveal_in_file_manager,
            // Live preview browser
            browser::browser_validate_url,
            browser::browser_get_listening_ports,
            browser::browser_register_shortcuts,
            browser::browser_unregister_shortcuts,
            // Updater (download .deb / AppImage, reveal in file manager)
            updater::get_install_source,
            updater::get_current_release_notes,
            updater::check_app_update,
            updater::download_app_update,
            updater::reveal_in_downloads,
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                let icon = tauri::include_image!("icons/icon.png");
                tracing::info!(
                    "embedded icon: {}x{} ({} bytes)",
                    icon.width(),
                    icon.height(),
                    icon.rgba().len()
                );
                match window.set_icon(icon) {
                    Ok(()) => tracing::info!("window.set_icon OK"),
                    Err(e) => tracing::error!("window.set_icon failed: {e}"),
                }
            } else {
                tracing::warn!("main window not found at setup");
            }

            let app_handle = app.handle().clone();
            let socket_path = config.hook_socket_path.clone();
            let transcripts_dir = config.transcripts_directory.clone();

            let hook_app = app_handle.clone();
            let hook_db = db.clone();
            let hook_plan = plan_state.clone();
            let hook_agents = agent_state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    start_hook_server(&socket_path, hook_app, hook_db, hook_plan, hook_agents).await
                {
                    tracing::error!("hook server error: {e}");
                }
            });

            // Drain HookEvents emitted by adapter event pumps (OpenCode SSE,
            // Pi JSONL tail) through the same dispatcher used for socket
            // events, so adapter-emitted events reach the frontend via the
            // existing Tauri event surface.
            hooks::server::spawn_adapter_event_consumer(
                app_handle.clone(),
                db.clone(),
                plan_state.clone(),
                agent_state.clone(),
                agent_event_rx,
            );

            // Live preview browser: localhost port scanner with hysteresis.
            // Emits `localhost:ports-changed` to the frontend; chips in the
            // status bar surface listening dev servers.
            let browser_app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                browser::run_port_scanner(browser_app).await;
            });

            // Plan watcher is dynamic: one notify::Watcher whose watch set
            // grows as sessions land in cwds the boot-time scan did not see
            // (e.g. worktrees outside the first workspace). Seed it with
            // every existing session's resolved plans dir so the first
            // session that's already in DB doesn't have to wait for a new
            // create_session to subscribe.
            let plan_watcher_arc: SharedPlanWatcher = match PlanWatcher::new(app_handle.clone()) {
                Ok(w) => std::sync::Arc::new(std::sync::Mutex::new(w)),
                Err(e) => {
                    tracing::error!("failed to construct plan watcher: {e}");
                    return Err(format!("plan watcher init: {e}").into());
                }
            };
            if let Ok(db_guard) = db.lock() {
                let workspaces = db_guard.get_workspaces().unwrap_or_default();
                for ws in workspaces {
                    let seeds: Vec<PathBuf> = ws
                        .sessions
                        .iter()
                        .map(|s| {
                            s.worktree_path
                                .clone()
                                .unwrap_or_else(|| ws.repo_path.clone())
                        })
                        .chain(std::iter::once(ws.repo_path.clone()))
                        .collect();
                    if let Ok(mut w) = plan_watcher_arc.lock() {
                        for cwd in seeds {
                            let dir = crate::agents::claude_code::resolve_cc_plans_directory(&cwd);
                            if let Err(e) = w.watch_dir(&dir) {
                                tracing::warn!(
                                    dir = %dir.display(),
                                    error = %e,
                                    "plan watcher seed watch failed"
                                );
                            }
                        }
                    }
                }
            }
            app.manage(plan_watcher_arc.clone());

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
                        tracing::info!("openspec watcher started on {}", openspec_dir.display());
                    }
                    Err(e) => tracing::warn!("failed to start openspec watcher: {e}"),
                }
            } else {
                tracing::info!(
                    "openspec directory does not exist yet, skipping watcher: {}",
                    openspec_dir.display()
                );
            }

            // Scratchpad watcher (lives for the lifetime of the app, owned
            // by the managed ScratchpadState so set_path can replace it).
            let scratchpad_state = app.state::<ScratchpadState>();
            match scratchpad::watcher::ScratchpadWatcher::spawn(
                scratchpad_root.clone(),
                scratchpad_state.own_writes.clone(),
                app_handle.clone(),
            ) {
                Ok(watcher) => {
                    if let Ok(mut guard) = scratchpad_state.watcher.lock() {
                        *guard = Some(watcher);
                    }
                    tracing::info!(
                        "scratchpad watcher started on {}",
                        scratchpad_root.display()
                    );
                }
                Err(e) => tracing::warn!("scratchpad watcher failed: {e}"),
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
