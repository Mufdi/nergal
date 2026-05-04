#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cluihud", about = "Desktop wrapper for Claude Code CLI")]
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
    /// running cluihud's view of available adapters. Sends a `control`
    /// message to the hook socket; no effect if cluihud is not running.
    RescanAgents,
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
    /// Configure Claude Code hooks in ~/.claude/settings.json
    Setup,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        None => cluihud::run(),

        Some(Commands::Hook { action }) => {
            let config = cluihud::config::Config::load();

            match action {
                HookAction::Send { .. } => {
                    if let Err(e) = cluihud::hooks::cli::send_hook_event(&config.hook_socket_path) {
                        eprintln!("cluihud hook send: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::InjectEdits => {
                    if let Err(e) = cluihud::hooks::cli::inject_edits() {
                        eprintln!("cluihud hook inject-edits: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::PlanReview => {
                    if let Err(e) = cluihud::hooks::cli::plan_review(&config.hook_socket_path) {
                        eprintln!("cluihud hook plan-review: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::AskUser => {
                    if let Err(e) = cluihud::hooks::cli::ask_user(&config.hook_socket_path) {
                        eprintln!("cluihud hook ask-user: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::Setup => {
                    if let Err(e) = cluihud::setup::run() {
                        eprintln!("cluihud hook setup: {e:#}");
                        std::process::exit(1);
                    }
                }
            }
        }

        Some(Commands::RescanAgents) => {
            let config = cluihud::config::Config::load();
            if let Err(e) = cluihud::hooks::cli::send_rescan_agents(&config.hook_socket_path) {
                eprintln!("cluihud rescan-agents: {e:#}");
                std::process::exit(1);
            }
        }
    }
}
