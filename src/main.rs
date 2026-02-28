#[allow(dead_code)]
mod app;
#[allow(dead_code)]
mod claude;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod hooks;
#[allow(dead_code)]
mod session;
mod setup;
#[allow(dead_code)]
mod tasks;
#[allow(dead_code)]
mod ui;
#[allow(dead_code)]
mod workspace;

use clap::{Parser, Subcommand};
use gpui::*;
use gpui_component::Root;
use gpui_ghostty_terminal::view::{
    Copy as TermCopy, Paste as TermPaste, SelectAll as TermSelectAll,
};

use crate::app::AppState;
use crate::config::Config;
use crate::workspace::{
    AcceptPlan, CloseTab, FocusPlan, FocusTasks, FocusTerminal, NewTab, NextTab, OpenSettings,
    PrevTab, RejectPlan, ToggleTheme,
};

use gpui_component::ActiveTheme as _;
use gpui_component::theme::{Theme, ThemeMode};

actions!(cluihud, [Quit]);

#[derive(Parser)]
#[command(name = "cluihud", about = "Desktop wrapper for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Hook subcommands for Claude Code integration
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Auto-configure Claude Code hooks for cluihud integration
    Setup,
}

#[derive(Subcommand)]
enum HookAction {
    /// Send a hook event to a running cluihud instance
    Send {
        /// Event name (session-start, stop, etc.) — for documentation only
        #[arg(value_name = "EVENT")]
        _event: String,
    },
    /// Inject plan edits into UserPromptSubmit (sync, reads/writes stdin/stdout)
    InjectEdits,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Hook { action }) => {
            match action {
                HookAction::Send { .. } => {
                    let config = Config::default();
                    if let Err(e) = hooks::cli::send_hook_event(&config.hook_socket_path) {
                        eprintln!("cluihud hook send: {e:#}");
                        std::process::exit(1);
                    }
                }
                HookAction::InjectEdits => {
                    if let Err(e) = hooks::cli::inject_edits() {
                        eprintln!("cluihud hook inject-edits: {e:#}");
                        std::process::exit(1);
                    }
                }
            }
            return;
        }
        Some(Commands::Setup) => {
            if let Err(e) = setup::run() {
                eprintln!("cluihud setup: {e:#}");
                std::process::exit(1);
            }
            return;
        }
        None => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cluihud=info".into()),
        )
        .init();

    let app = Application::new().with_assets(gpui_component_assets::Assets);

    app.run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("ctrl-t", NewTab, None),
            KeyBinding::new("ctrl-w", CloseTab, None),
            KeyBinding::new("ctrl-tab", NextTab, None),
            KeyBinding::new("ctrl-shift-tab", PrevTab, None),
            KeyBinding::new("ctrl-shift-t", ToggleTheme, None),
            KeyBinding::new("ctrl-shift-c", TermCopy, Some("Terminal")),
            KeyBinding::new("ctrl-shift-v", TermPaste, Some("Terminal")),
            KeyBinding::new("ctrl-shift-a", TermSelectAll, Some("Terminal")),
            KeyBinding::new("ctrl-1", FocusTerminal, Some("Workspace")),
            KeyBinding::new("ctrl-2", FocusPlan, Some("Workspace")),
            KeyBinding::new("ctrl-3", FocusTasks, Some("Workspace")),
            KeyBinding::new("ctrl-y", AcceptPlan, Some("Workspace")),
            KeyBinding::new("ctrl-n", RejectPlan, Some("Workspace")),
            KeyBinding::new("ctrl-comma", OpenSettings, Some("Workspace")),
        ]);

        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.on_action(|_: &ToggleTheme, cx: &mut App| {
            let new_mode = if cx.theme().mode.is_dark() {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            };
            Theme::change(new_mode, None, cx);
        });

        let config = Config::load();

        let initial_theme = if config.theme_mode == "light" {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(initial_theme, None, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| AppState::new(config, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open window");
    });
}
