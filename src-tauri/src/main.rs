#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nergal", about = "Desktop wrapper for Claude Code CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Hook subcommands (used by Claude Code hooks config)
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Re-run filesystem detection for installed agents and refresh the
    /// running nergal's view of available adapters. Sends a `control`
    /// message to the hook socket; no effect if nergal is not running.
    RescanAgents,
    /// Detached background runner: drain pending session-end markers (MOC +
    /// reverse backlinks). Spawned by the app, not invoked by hand.
    PostSession,
    /// stdio MCP shim: relays JSON-RPC between an agent and the nergal MCP
    /// daemon. Registered into agent MCP configs; not invoked by hand.
    Mcp,
}

#[derive(Subcommand)]
enum HookAction {
    /// Forward a hook event from stdin JSON to the Unix socket
    Send {
        /// Event label (informational only, actual data comes from stdin)
        #[arg(trailing_var_arg = true)]
        _args: Vec<String>,
    },
    /// Inject plan edits into the UserPromptSubmit hook (deprecated)
    InjectEdits,
    /// Synchronous plan review for PermissionRequest[ExitPlanMode] hook
    PlanReview,
    /// Synchronous AskUserQuestion interception via GUI
    AskUser,
    /// Non-blocking Notification forwarder (permission prompts, idle prompts, …)
    Notification,
    /// Configure Claude Code hooks in ~/.claude/settings.json
    Setup,
}

fn main() {
    // clap parsea argv[1] como subcomando y aborta con URLs; saltarlo deja que
    // tauri-plugin-{deep-link,single-instance} consuman la URL desde env::args.
    if std::env::args()
        .nth(1)
        .is_some_and(|a| a.starts_with("nergal://"))
    {
        nergal::run();
        return;
    }

    let cli = Cli::parse();

    match cli.command {
        None => nergal::run(),

        Some(Commands::Hook { action }) => {
            // Always derive the socket path from the per-user IPC resolver
            // (same getuid()-keyed path the GUI binds). The config path is
            // only a fallback in case the resolver fails (misconfigured host).
            let socket_path = nergal::platform::hook_socket_path().unwrap_or_else(|e| {
                eprintln!("nergal: IPC dir resolver failed: {e:#}; using config path");
                nergal::config::Config::load().hook_socket_path.clone()
            });

            match action {
                HookAction::Send { .. } => {
                    if let Err(e) = nergal::hooks::cli::send_hook_event(&socket_path) {
                        eprintln!("nergal hook send: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::InjectEdits => {
                    if let Err(e) = nergal::hooks::cli::inject_edits() {
                        eprintln!("nergal hook inject-edits: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::PlanReview => {
                    if let Err(e) = nergal::hooks::cli::plan_review(&socket_path) {
                        eprintln!("nergal hook plan-review: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::AskUser => {
                    if let Err(e) = nergal::hooks::cli::ask_user(&socket_path) {
                        eprintln!("nergal hook ask-user: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::Notification => {
                    if let Err(e) = nergal::hooks::cli::notification(&socket_path) {
                        eprintln!("nergal hook notification: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::Setup => {
                    if let Err(e) = nergal::setup::run() {
                        eprintln!("nergal hook setup: {e:#}");
                        std::process::exit(1);
                    }
                }
            }
        }

        Some(Commands::RescanAgents) => {
            let socket_path = nergal::platform::hook_socket_path().unwrap_or_else(|e| {
                eprintln!("nergal: IPC dir resolver failed: {e:#}; using config path");
                nergal::config::Config::load().hook_socket_path.clone()
            });
            if let Err(e) = nergal::hooks::cli::send_rescan_agents(&socket_path) {
                eprintln!("nergal rescan-agents: {e:#}");
                std::process::exit(1);
            }
        }

        Some(Commands::PostSession) => {
            if let Err(e) = nergal::obsidian::post_session::run() {
                eprintln!("nergal post-session: {e:#}");
                std::process::exit(1);
            }
        }

        Some(Commands::Mcp) => {
            if let Err(e) = nergal::mcp::shim::run() {
                eprintln!("nergal mcp: {e:#}");
                std::process::exit(1);
            }
        }
    }
}
