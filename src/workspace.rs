use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use gpui::*;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::notification::{Notification, NotificationType};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::{ActiveTheme as _, IconName, Sizable as _};

use crate::claude::cost::parse_cost_from_transcript;
use crate::claude::plan::{PlanEvent, PlanWatcher};
use crate::claude::transcript::TranscriptWatcher;
use crate::config::Config;
use crate::hooks::events::HookEvent;
use crate::hooks::server::start_hook_server;
use crate::session::Session;
use crate::tasks::transcript_parser::parse_tasks_from_transcript;
use crate::ui::nav_sidebar::{NavSidebar, NavSidebarEvent, SessionEntry};
use crate::ui::plan_panel::{PlanAction, PlanPanel, PlanVisibilityEvent};
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
        FocusActivity,
        AcceptPlan,
        RejectPlan,
        OpenSettings,
        TogglePlan,
        ToggleSidebar,
    ]
);

pub struct Workspace {
    sessions: Vec<Session>,
    active_index: usize,
    next_session_number: usize,
    pub status_bar: Entity<StatusBar>,
    nav_sidebar: Entity<NavSidebar>,
    plan_visible: bool,
    config: Config,
    transcripts_directory: PathBuf,
    focus_handle: FocusHandle,
    cost_tx: std_mpsc::Sender<crate::claude::cost::CostSummary>,
    tasks_tx: std_mpsc::Sender<crate::tasks::TaskStore>,
    _plan_watcher: Option<PlanWatcher>,
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
        let nav_sidebar = cx.new(NavSidebar::new);

        // Subscribe to sidebar events
        cx.subscribe(
            &nav_sidebar,
            |ws: &mut Workspace, _sidebar, event, cx| match event {
                NavSidebarEvent::SwitchSession(idx) => {
                    ws.active_index = *idx;
                    ws.sync_sidebar(cx);
                    cx.notify();
                }
                NavSidebarEvent::ToggleCollapse => {
                    cx.notify();
                }
            },
        )
        .detach();

        // First session
        let session = Session::new("Session 1".into(), plans_directory.clone(), window, cx);
        Self::subscribe_plan_to_terminal(&session.plan, &session.terminal, cx);
        Self::subscribe_plan_visibility(&session.plan, cx);

        let (cost_tx, cost_rx) = std_mpsc::channel::<crate::claude::cost::CostSummary>();
        let (tasks_tx, tasks_rx) = std_mpsc::channel::<crate::tasks::TaskStore>();

        let cost_status_bar = status_bar.clone();
        let this_weak = cx.entity().downgrade();
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;

                    let mut latest_cost = None;
                    while let Ok(cost) = cost_rx.try_recv() {
                        latest_cost = Some(cost);
                    }

                    let mut latest_tasks = None;
                    while let Ok(store) = tasks_rx.try_recv() {
                        latest_tasks = Some(store);
                    }

                    if latest_cost.is_none() && latest_tasks.is_none() {
                        continue;
                    }

                    cx.update(|_, cx| {
                        if let Some(cost) = latest_cost {
                            cost_status_bar.update(cx, |bar, cx| {
                                bar.update_cost(cost, cx);
                            });
                        }
                        if let Some(store) = latest_tasks
                            && let Some(ws) = this_weak.upgrade()
                        {
                            ws.update(cx, |ws, cx| {
                                let tasks = ws.active_session().tasks.clone();
                                tasks.update(cx, |panel, cx| {
                                    panel.replace_store(store, cx);
                                });
                            });
                        }
                    })
                    .ok();
                }
            })
            .detach();

        let focus_handle = cx.focus_handle();

        // Plan directory watcher — detects .md changes without depending on hooks
        let (plan_watcher, plan_rx) = Self::start_plan_watcher(&plans_directory);

        let status_bar_handle = status_bar.clone();
        let this = cx.entity().downgrade();

        // Single event loop for hooks + plan watcher (shared window context ensures re-renders)
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;

                    let mut hook_events = Vec::new();
                    while let Ok(event) = std_rx.try_recv() {
                        hook_events.push(event);
                    }

                    let mut plan_changed = None;
                    while let Ok(path) = plan_rx.try_recv() {
                        plan_changed = Some(path);
                    }

                    if hook_events.is_empty() && plan_changed.is_none() {
                        continue;
                    }

                    cx.update(|window, cx| {
                        for event in &hook_events {
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

                        if let Some(path) = plan_changed {
                            tracing::info!("plan file changed: {}", path.display());
                            if let Some(workspace) = this.upgrade() {
                                workspace.update(cx, |ws, cx| {
                                    let plan = ws.active_session().plan.clone();
                                    plan.update(cx, |p, cx| p.on_plan_ready(cx));
                                });
                            }
                        }
                    })
                    .ok();
                }
            })
            .detach();

        // Initialize sidebar with first session
        nav_sidebar.update(cx, |sb, cx| {
            sb.set_sessions(
                vec![SessionEntry {
                    label: "Session 1".into(),
                    active: true,
                }],
                cx,
            );
        });

        Self {
            sessions: vec![session],
            active_index: 0,
            next_session_number: 2,
            status_bar,
            nav_sidebar,
            plan_visible: false,
            config,
            transcripts_directory,
            focus_handle,
            cost_tx,
            tasks_tx,
            _plan_watcher: plan_watcher,
        }
    }

    /// Creates a PlanWatcher and bridges events to a std_mpsc channel.
    fn start_plan_watcher(plans_dir: &Path) -> (Option<PlanWatcher>, std_mpsc::Receiver<PathBuf>) {
        let (std_tx, std_rx) = std_mpsc::channel::<PathBuf>();

        if !plans_dir.exists()
            && let Err(e) = std::fs::create_dir_all(plans_dir)
        {
            tracing::warn!("failed to create plans directory: {e:#}");
            return (None, std_rx);
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<PlanEvent>(64);

        // Bridge tokio → std_mpsc in a background thread
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for plan watcher");

            rt.block_on(async {
                while let Some(PlanEvent::Updated(path)) = rx.recv().await {
                    if std_tx.send(path).is_err() {
                        break;
                    }
                }
            });
        });

        match PlanWatcher::new(plans_dir, tx) {
            Ok(watcher) => {
                tracing::info!("plan watcher started for {}", plans_dir.display());
                (Some(watcher), std_rx)
            }
            Err(e) => {
                tracing::warn!("failed to start plan watcher: {e:#}");
                (None, std_rx)
            }
        }
    }

    fn subscribe_plan_to_terminal(
        plan: &Entity<PlanPanel>,
        terminal: &Entity<TerminalPanel>,
        cx: &mut App,
    ) {
        let terminal_handle = terminal.clone();
        cx.subscribe(plan, move |_plan, action: &PlanAction, cx| {
            let bytes: Vec<u8> = match action {
                PlanAction::AcceptClearContext => {
                    b"Proceed with the plan. Clear context and auto-accept all edits.\n".to_vec()
                }
                PlanAction::AcceptAutoEdits => {
                    b"Proceed with the plan. Auto-accept all edits.\n".to_vec()
                }
                PlanAction::AcceptManualEdits => {
                    b"Proceed with the plan. I want to manually approve each edit.\n".to_vec()
                }
                PlanAction::Feedback(text) => format!("{text}\n").into_bytes(),
            };
            terminal_handle.update(cx, |t, _| {
                t.write_to_pty(&bytes);
            });
        })
        .detach();
    }

    /// Subscribe to PlanVisibilityEvent to auto-show/hide the plan panel.
    fn subscribe_plan_visibility(plan: &Entity<PlanPanel>, cx: &mut Context<Self>) {
        cx.subscribe(
            plan,
            |ws: &mut Workspace, _plan, event: &PlanVisibilityEvent, cx| {
                match event {
                    PlanVisibilityEvent::PlanLoaded => ws.plan_visible = true,
                    PlanVisibilityEvent::PlanDismissed => ws.plan_visible = false,
                }
                cx.notify();
            },
        )
        .detach();
    }

    /// Sync sidebar session list with current state.
    fn sync_sidebar(&self, cx: &mut Context<Self>) {
        let entries: Vec<SessionEntry> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| SessionEntry {
                label: s.label.clone(),
                active: i == self.active_index,
            })
            .collect();
        let sidebar = self.nav_sidebar.clone();
        sidebar.update(cx, |sb, cx| sb.set_sessions(entries, cx));
    }

    pub fn add_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = format!("Session {}", self.next_session_number);
        self.next_session_number += 1;

        let plans_dir = self.config.plans_directory.clone();
        let session = Session::new(label, plans_dir, window, cx);
        Self::subscribe_plan_to_terminal(&session.plan, &session.terminal, cx);
        Self::subscribe_plan_visibility(&session.plan, cx);

        self.sessions.push(session);
        self.active_index = self.sessions.len() - 1;
        self.sync_sidebar(cx);
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
        self.sync_sidebar(cx);
        cx.notify();
    }

    fn active_session(&self) -> &Session {
        &self.sessions[self.active_index]
    }

    pub fn next_tab(&mut self, cx: &mut Context<Self>) {
        if self.sessions.len() > 1 {
            self.active_index = (self.active_index + 1) % self.sessions.len();
            self.sync_sidebar(cx);
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
            self.sync_sidebar(cx);
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
            HookEvent::Stop {
                session_id,
                transcript_path,
                ..
            } => {
                tracing::info!("session stopped: {session_id}");
                // Final re-parse to capture any last TaskUpdate writes
                if let Some(tp) = transcript_path {
                    let path = PathBuf::from(tp);
                    if path.exists() {
                        let store = parse_tasks_from_transcript(&path);
                        if !store.is_empty() {
                            tracing::info!(
                                "final transcript re-parse: {} visible tasks",
                                store.visible_count()
                            );
                            let tasks = session.tasks.clone();
                            tasks.update(cx, |panel, cx| {
                                panel.replace_store(store, cx);
                            });
                        }
                    }
                }
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
                tracing::info!(
                    "routing PostToolUse {tool_name}: {}",
                    serde_json::to_string(tool_input).unwrap_or_default()
                );
                let tasks = session.tasks.clone();
                tasks.update(cx, |panel, cx| {
                    panel.handle_tool_event(tool_name, tool_input, cx);
                });
            }
            HookEvent::TaskCompleted {
                task_id: Some(task_id),
                ..
            } => {
                tracing::info!("TaskCompleted: marking task {task_id} as completed");
                let tasks = session.tasks.clone();
                let update = serde_json::json!({
                    "taskId": task_id,
                    "status": "completed"
                });
                tasks.update(cx, |panel, cx| {
                    panel.handle_tool_event("TaskUpdate", &update, cx);
                });
            }
            _ => {}
        }

        // Push all events to the activity log
        let activity = self.sessions[idx].activity_log.clone();
        activity.update(cx, |log, cx| {
            log.push_from_hook(event, cx);
        });
    }

    fn start_transcript_watcher(&mut self, session_idx: usize) {
        let session = &mut self.sessions[session_idx];
        if session.transcript_watcher.is_some() {
            return;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let cost_tx = self.cost_tx.clone();
        let tasks_tx = self.tasks_tx.clone();

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
                            if !store.is_empty() {
                                let _ = tasks_tx.send(store);
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

impl Workspace {
    fn render_plan_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let icon = if self.plan_visible {
            IconName::PanelRightClose
        } else {
            IconName::PanelRightOpen
        };

        div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(24.))
            .h_full()
            .flex_shrink_0()
            .cursor_pointer()
            .border_l_1()
            .border_color(theme.muted.opacity(0.3))
            .hover(|s| s.bg(theme.sidebar_accent.opacity(0.5)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.plan_visible = !this.plan_visible;
                    cx.notify();
                }),
            )
            .child(
                gpui_component::Icon::new(icon)
                    .size_4()
                    .text_color(theme.muted_foreground),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.active_session();
        let plan_visible = self.plan_visible;

        let terminal = session.terminal.clone();
        let plan = session.plan.clone();
        let tasks = session.tasks.clone();
        let activity = session.activity_log.clone();
        let bg_color = cx.theme().background;

        // Left panels: Tasks + Activity stacked vertically
        let left_panels = v_resizable("left-panels")
            .child(
                resizable_panel()
                    .size(px(250.))
                    .size_range(px(100.)..px(500.))
                    .child(tasks),
            )
            .child(
                resizable_panel()
                    .size(px(250.))
                    .size_range(px(80.)..px(500.))
                    .child(activity),
            );

        // Main content: resizable horizontal split [left-panels | terminal]
        let mut main_area = h_resizable("workspace-main")
            .child(
                resizable_panel()
                    .size(px(240.))
                    .size_range(px(160.)..px(400.))
                    .child(left_panels),
            )
            .child(resizable_panel().size(px(800.)).child(terminal));

        // Conditionally add plan panel as resizable
        if plan_visible {
            main_area = main_area.child(
                resizable_panel()
                    .size(px(420.))
                    .size_range(px(250.)..px(700.))
                    .child(plan),
            );
        }

        div()
            .track_focus(&self.focus_handle)
            .key_context("Workspace")
            .flex()
            .flex_col()
            .size_full()
            .bg(bg_color)
            .on_action(cx.listener(|this, _: &NewTab, window, cx| {
                this.add_session(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, _window, cx| {
                let idx = this.active_index;
                this.close_session(idx, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, _window, cx| {
                this.next_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &PrevTab, _window, cx| {
                this.prev_tab(cx);
            }))
            .on_action(cx.listener(|this, _: &FocusTerminal, window, cx| {
                this.active_session().focus_terminal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusPlan, window, cx| {
                this.plan_visible = true;
                this.active_session().focus_plan(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusTasks, window, cx| {
                this.active_session().focus_tasks(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusActivity, window, cx| {
                this.active_session().focus_activity(window, cx);
            }))
            .on_action(cx.listener(|this, _: &AcceptPlan, _window, cx| {
                let plan = this.active_session().plan.clone();
                plan.update(cx, |p, cx| p.accept(PlanAction::AcceptClearContext, cx));
            }))
            .on_action(cx.listener(|this, _: &RejectPlan, _window, cx| {
                let plan = this.active_session().plan.clone();
                plan.update(cx, |p, cx| p.accept(PlanAction::AcceptManualEdits, cx));
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings(window, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePlan, _, cx| {
                this.plan_visible = !this.plan_visible;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                let sidebar = this.nav_sidebar.clone();
                sidebar.update(cx, |sb, cx| sb.toggle_collapse(cx));
            }))
            .child(self.render_tab_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_grow()
                    .min_h_0()
                    .bg(bg_color)
                    .p(px(4.))
                    .gap(px(4.))
                    // Nav sidebar
                    .child(self.nav_sidebar.clone())
                    // Main resizable area
                    .child(main_area)
                    // Plan toggle button on right edge
                    .child(self.render_plan_toggle(cx)),
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
