use std::any::TypeId;
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use gpui::*;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::notification::{Notification, NotificationType};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _};

use crate::claude::cost::parse_cost_from_transcript;
use crate::claude::transcript::TranscriptWatcher;
use crate::config::Config;
use crate::hooks::events::HookEvent;
use crate::hooks::server::start_hook_server;
use crate::session::Session;
use crate::tasks::transcript_parser::parse_tasks_from_transcript;
use crate::ui::plan_panel::{PlanAction, PlanPanel};
use crate::ui::settings_panel::SettingsPanel;
use crate::ui::status_bar::StatusBar;
use crate::ui::terminal_panel::TerminalPanel;

actions!(
    workspace,
    [
        NewTab,
        CloseTab,
        NextTab,
        PrevTab,
        ToggleTheme,
        FocusTerminal,
        FocusPlan,
        FocusTasks,
        AcceptPlan,
        RejectPlan,
        OpenSettings,
    ]
);

pub struct Workspace {
    sessions: Vec<Session>,
    active_index: usize,
    next_session_number: usize,
    pub status_bar: Entity<StatusBar>,
    config: Config,
    transcripts_directory: PathBuf,
    focus_handle: FocusHandle,
    cost_tx: std_mpsc::Sender<crate::claude::cost::CostSummary>,
}

impl Workspace {
    pub fn new(config: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let socket_path = config.hook_socket_path.clone();
        let transcripts_directory = config.transcripts_directory.clone();
        let plans_directory = config.plans_directory.clone();

        let (std_tx, std_rx) = std_mpsc::channel::<HookEvent>();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");

            rt.block_on(async {
                let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::channel::<HookEvent>(64);

                tokio::spawn(async move {
                    if let Err(e) = start_hook_server(&socket_path, tokio_tx).await {
                        tracing::error!("hook server error: {e:#}");
                    }
                });

                while let Some(event) = tokio_rx.recv().await {
                    if std_tx.send(event).is_err() {
                        break;
                    }
                }
            });
        });

        let status_bar = cx.new(StatusBar::new);

        // First session
        let session = Session::new("Session 1".into(), plans_directory, window, cx);
        Self::subscribe_plan_to_terminal(&session.plan, &session.terminal, cx);

        let (cost_tx, cost_rx) = std_mpsc::channel::<crate::claude::cost::CostSummary>();

        let cost_status_bar = status_bar.clone();
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;

                    let mut latest = None;
                    while let Ok(cost) = cost_rx.try_recv() {
                        latest = Some(cost);
                    }

                    let Some(cost) = latest else { continue };

                    cx.update(|_, cx| {
                        cost_status_bar.update(cx, |bar, cx| {
                            bar.update_cost(cost, cx);
                        });
                    })
                    .ok();
                }
            })
            .detach();

        let status_bar_handle = status_bar.clone();
        let this = cx.entity().downgrade();

        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;

                    let mut events = Vec::new();
                    while let Ok(event) = std_rx.try_recv() {
                        events.push(event);
                    }
                    if events.is_empty() {
                        continue;
                    }

                    cx.update(|window, cx| {
                        for event in &events {
                            status_bar_handle.update(cx, |bar, cx| {
                                bar.handle_hook_event(event, cx);
                            });

                            if let Some(workspace) = this.upgrade() {
                                workspace.update(cx, |ws, cx| {
                                    ws.route_hook_event(event, cx);
                                });
                            }

                            push_event_notification(event, window, cx);
                        }
                    })
                    .ok();
                }
            })
            .detach();

        cx.on_action(
            TypeId::of::<NewTab>(),
            window,
            |this, _, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    this.add_session(window, cx);
                }
            },
        );
        cx.on_action(TypeId::of::<CloseTab>(), window, |this, _, phase, _, cx| {
            if phase == DispatchPhase::Bubble {
                let idx = this.active_index;
                this.close_session(idx, cx);
            }
        });
        cx.on_action(TypeId::of::<NextTab>(), window, |this, _, phase, _, cx| {
            if phase == DispatchPhase::Bubble {
                this.next_tab(cx);
            }
        });
        cx.on_action(TypeId::of::<PrevTab>(), window, |this, _, phase, _, cx| {
            if phase == DispatchPhase::Bubble {
                this.prev_tab(cx);
            }
        });
        cx.on_action(
            TypeId::of::<FocusTerminal>(),
            window,
            |this, _, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    this.active_session().focus_terminal(window, cx);
                }
            },
        );
        cx.on_action(
            TypeId::of::<FocusPlan>(),
            window,
            |this, _, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    this.active_session().focus_plan(window, cx);
                }
            },
        );
        cx.on_action(
            TypeId::of::<FocusTasks>(),
            window,
            |this, _, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    this.active_session().focus_tasks(window, cx);
                }
            },
        );
        cx.on_action(
            TypeId::of::<AcceptPlan>(),
            window,
            |this, _, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let plan = this.active_session().plan.clone();
                    plan.update(cx, |p, cx| p.accept_plan(cx));
                }
            },
        );
        cx.on_action(
            TypeId::of::<RejectPlan>(),
            window,
            |this, _, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let plan = this.active_session().plan.clone();
                    plan.update(cx, |p, cx| p.reject_with_edits(cx));
                }
            },
        );

        cx.on_action(
            TypeId::of::<OpenSettings>(),
            window,
            |this, _, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    this.open_settings(window, cx);
                }
            },
        );

        let focus_handle = cx.focus_handle();

        Self {
            sessions: vec![session],
            active_index: 0,
            next_session_number: 2,
            status_bar,
            config,
            transcripts_directory,
            focus_handle,
            cost_tx,
        }
    }

    fn subscribe_plan_to_terminal(
        plan: &Entity<PlanPanel>,
        terminal: &Entity<TerminalPanel>,
        cx: &mut App,
    ) {
        let terminal_handle = terminal.clone();
        cx.subscribe(plan, move |_plan, action: &PlanAction, cx| match action {
            PlanAction::Accept => {
                terminal_handle.update(cx, |t, _| t.write_to_pty(b"y\n"));
            }
            PlanAction::Reject => {
                terminal_handle.update(cx, |t, _| t.write_to_pty(b"n\n"));
            }
        })
        .detach();
    }

    pub fn add_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = format!("Session {}", self.next_session_number);
        self.next_session_number += 1;

        let plans_dir = self.config.plans_directory.clone();
        let session = Session::new(label, plans_dir, window, cx);
        Self::subscribe_plan_to_terminal(&session.plan, &session.terminal, cx);

        self.sessions.push(session);
        self.active_index = self.sessions.len() - 1;
        cx.notify();
    }

    pub fn close_session(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.sessions.len() <= 1 {
            return;
        }
        self.sessions.remove(index);
        if self.active_index >= self.sessions.len() {
            self.active_index = self.sessions.len() - 1;
        } else if self.active_index > index {
            self.active_index -= 1;
        }
        cx.notify();
    }

    fn active_session(&self) -> &Session {
        &self.sessions[self.active_index]
    }

    pub fn next_tab(&mut self, cx: &mut Context<Self>) {
        if self.sessions.len() > 1 {
            self.active_index = (self.active_index + 1) % self.sessions.len();
            cx.notify();
        }
    }

    pub fn prev_tab(&mut self, cx: &mut Context<Self>) {
        if self.sessions.len() > 1 {
            self.active_index = if self.active_index == 0 {
                self.sessions.len() - 1
            } else {
                self.active_index - 1
            };
            cx.notify();
        }
    }

    /// Route a hook event to the matching session by session_id.
    fn route_hook_event(&mut self, event: &HookEvent, cx: &mut Context<Self>) {
        let event_session_id = event.session_id();

        // Find session by id
        let session_idx = self
            .sessions
            .iter()
            .position(|s| s.id.as_deref() == Some(event_session_id));

        let idx = match (session_idx, event) {
            (Some(idx), _) => idx,
            // SessionStart with no match: assign to active session if unbound, else create new
            (None, HookEvent::SessionStart { session_id }) => {
                let active = &mut self.sessions[self.active_index];
                if active.id.is_none() {
                    active.id = Some(session_id.clone());
                    tracing::info!("bound session_id={session_id} to active tab");
                    self.active_index
                } else {
                    // All sessions bound — would need a new tab, but for now assign to active
                    tracing::warn!(
                        "new session_id={session_id} but active tab already bound, overwriting"
                    );
                    active.id = Some(session_id.clone());
                    self.active_index
                }
            }
            (None, _) => {
                tracing::debug!("ignoring event for unknown session_id={}", event_session_id);
                return;
            }
        };

        let session = &mut self.sessions[idx];

        // Dispatch to session panels
        match event {
            HookEvent::SessionStart { session_id } => {
                tracing::info!("session started: {session_id}");
                self.start_transcript_watcher(idx);
            }
            HookEvent::SessionEnd { session_id } => {
                tracing::info!("session ended: {session_id}");
                session.id = None;
                session.transcript_watcher = None;
            }
            HookEvent::Stop { session_id, .. } => {
                tracing::info!("session stopped: {session_id}");
                session.id = None;
                session.transcript_watcher = None;
            }
            HookEvent::PreToolUse { tool_name, .. } if tool_name == "ExitPlanMode" => {
                let plan = session.plan.clone();
                plan.update(cx, |panel, cx| {
                    panel.on_plan_ready(cx);
                });
            }
            HookEvent::PostToolUse {
                tool_name,
                tool_input,
                ..
            } if tool_name == "TaskCreate" || tool_name == "TaskUpdate" => {
                let tasks = session.tasks.clone();
                tasks.update(cx, |panel, cx| {
                    panel.handle_tool_event(tool_name, tool_input, cx);
                });
            }
            _ => {}
        }
    }

    fn start_transcript_watcher(&mut self, session_idx: usize) {
        let session = &mut self.sessions[session_idx];
        if session.transcript_watcher.is_some() {
            return;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let cost_tx = self.cost_tx.clone();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for transcript");

            rt.block_on(async {
                while let Some(event) = rx.recv().await {
                    match event {
                        crate::claude::transcript::TranscriptEvent::Updated(path) => {
                            tracing::debug!("transcript updated: {}", path.display());
                            let store = parse_tasks_from_transcript(&path);
                            let count = store.tasks().len();
                            if count > 0 {
                                tracing::info!(
                                    "transcript re-parse found {count} tasks from {}",
                                    path.display()
                                );
                            }
                            let cost = parse_cost_from_transcript(&path);
                            let _ = cost_tx.send(cost);
                        }
                    }
                }
            });
        });

        match TranscriptWatcher::new(&self.transcripts_directory, tx) {
            Ok(watcher) => {
                tracing::info!(
                    "transcript watcher started for {}",
                    self.transcripts_directory.display()
                );
                session.transcript_watcher = Some(watcher);
            }
            Err(e) => {
                tracing::warn!("failed to start transcript watcher: {e:#}");
            }
        }
    }

    fn open_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let settings_view = cx.new(|cx| SettingsPanel::new(config, window, cx));

        window.open_sheet(cx, move |sheet, _window, _cx| {
            sheet
                .title("Settings")
                .size(px(400.))
                .child(settings_view.clone())
        });
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Only show tab bar when there are multiple sessions
        if self.sessions.len() <= 1 {
            return div();
        }

        let mut tab_bar = TabBar::new("session-tabs")
            .selected_index(self.active_index)
            .on_click(cx.listener(|this, idx: &usize, _, cx| {
                this.active_index = *idx;
                cx.notify();
            }));

        for (i, session) in self.sessions.iter().enumerate() {
            let close_idx = i;
            let tab = Tab::new().label(session.label.clone()).suffix(
                Button::new(SharedString::from(format!("close-tab-{i}")))
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_session(close_idx, cx);
                    })),
            );
            tab_bar = tab_bar.child(tab);
        }

        tab_bar = tab_bar.suffix(
            Button::new("new-tab")
                .icon(IconName::Plus)
                .ghost()
                .xsmall()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.add_session(window, cx);
                })),
        );

        div().child(tab_bar)
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.active_session();

        div()
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_tab_bar(cx))
            .child(
                h_resizable("workspace-panels")
                    .child(
                        resizable_panel()
                            .size(px(480.))
                            .size_range(px(200.)..px(800.))
                            .child(session.terminal.clone()),
                    )
                    .child(
                        resizable_panel()
                            .size(px(420.))
                            .size_range(px(200.)..px(600.))
                            .child(session.plan.clone()),
                    )
                    .child(
                        resizable_panel()
                            .size(px(300.))
                            .size_range(px(150.)..px(500.))
                            .child(session.tasks.clone()),
                    ),
            )
            .child(self.status_bar.clone())
    }
}

/// Push a notification for notable hook events.
fn push_event_notification(event: &HookEvent, window: &mut Window, cx: &mut App) {
    let notification = match event {
        HookEvent::PreToolUse { tool_name, .. } if tool_name == "ExitPlanMode" => {
            Notification::new()
                .title("Plan Ready")
                .message("A plan is ready for review.")
                .with_type(NotificationType::Info)
        }
        HookEvent::TaskCompleted { .. } => Notification::new()
            .title("Task Completed")
            .message("A task has finished.")
            .with_type(NotificationType::Success),
        HookEvent::Stop {
            stop_reason: Some(reason),
            ..
        } => Notification::new()
            .title("Session Stopped")
            .message(SharedString::from(reason.clone()))
            .with_type(NotificationType::Warning),
        HookEvent::SessionEnd { .. } => Notification::new()
            .title("Session Ended")
            .message("The Claude session has ended.")
            .with_type(NotificationType::Info),
        _ => return,
    };

    window.push_notification(notification, cx);
}
