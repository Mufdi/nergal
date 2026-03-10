use std::collections::VecDeque;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{Icon, IconName};

use crate::hooks::events::HookEvent;

const MAX_ENTRIES: usize = 200;

/// Kind of activity event displayed in the log.
#[derive(Debug, Clone)]
pub enum ActivityKind {
    ToolUse,
    FileModified,
    SessionEvent,
    TaskEvent,
}

/// A single entry in the activity log.
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub kind: ActivityKind,
    pub summary: String,
    pub file_path: Option<String>,
}

/// Panel showing a chronological log of Claude session activity.
pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
    focus_handle: FocusHandle,
}

impl ActivityLog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus this panel.
    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    /// Push a new entry, evicting the oldest if at capacity.
    pub fn push_entry(&mut self, entry: ActivityEntry, cx: &mut Context<Self>) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        cx.notify();
    }

    /// Convert a hook event into an activity entry and push it.
    pub fn push_from_hook(&mut self, event: &HookEvent, cx: &mut Context<Self>) {
        let entry = match event {
            HookEvent::SessionStart { session_id } => ActivityEntry {
                kind: ActivityKind::SessionEvent,
                summary: format!("Session started: {session_id}"),
                file_path: None,
            },
            HookEvent::SessionEnd { session_id } => ActivityEntry {
                kind: ActivityKind::SessionEvent,
                summary: format!("Session ended: {session_id}"),
                file_path: None,
            },
            HookEvent::PreToolUse { tool_name, .. } => ActivityEntry {
                kind: ActivityKind::ToolUse,
                summary: format!("Tool: {tool_name}"),
                file_path: None,
            },
            HookEvent::PostToolUse {
                tool_name,
                tool_input,
                ..
            } => {
                let file_path = extract_file_path(tool_name, tool_input);
                let kind = if file_path.is_some() {
                    ActivityKind::FileModified
                } else {
                    ActivityKind::ToolUse
                };
                ActivityEntry {
                    kind,
                    summary: format!("{tool_name} completed"),
                    file_path,
                }
            }
            HookEvent::Stop { stop_reason, .. } => ActivityEntry {
                kind: ActivityKind::SessionEvent,
                summary: format!("Stopped: {}", stop_reason.as_deref().unwrap_or("unknown")),
                file_path: None,
            },
            HookEvent::TaskCompleted {
                task_subject,
                task_id,
                ..
            } => {
                let label = task_subject
                    .as_deref()
                    .or(task_id.as_deref())
                    .unwrap_or("unknown");
                ActivityEntry {
                    kind: ActivityKind::TaskEvent,
                    summary: format!("Task completed: {label}"),
                    file_path: None,
                }
            }
            HookEvent::UserPromptSubmit { .. } => ActivityEntry {
                kind: ActivityKind::SessionEvent,
                summary: "User prompt submitted".to_string(),
                file_path: None,
            },
        };
        self.push_entry(entry, cx);
    }
}

/// Extract a file path from tool_input for file-modifying tools.
fn extract_file_path(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    match tool_name {
        "Write" | "Edit" | "Read" => tool_input
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    }
}

impl Render for ActivityLog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .child(Icon::new(IconName::Inbox).size_4().text_color(theme.accent))
            .child(
                div()
                    .text_color(theme.foreground)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(12.))
                    .child("ACTIVITY"),
            )
            .when(!self.entries.is_empty(), |this| {
                this.child(
                    div()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(8.))
                        .bg(theme.muted)
                        .text_color(theme.muted_foreground)
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{}", self.entries.len())),
                )
            });

        let root = div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .mt(px(2.))
            .bg(theme.sidebar)
            .rounded(theme.radius)
            .overflow_hidden()
            .child(header);

        if self.entries.is_empty() {
            return root.child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(
                        Icon::new(IconName::Inbox)
                            .size_8()
                            .text_color(theme.muted_foreground.opacity(0.3)),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .text_size(px(12.))
                            .child("No activity yet"),
                    ),
            );
        }

        let mut list = div()
            .flex()
            .flex_col()
            .overflow_y_scrollbar()
            .flex_grow()
            .px(px(6.))
            .gap(px(1.));

        for entry in &self.entries {
            let (dot_color, icon_char) = match entry.kind {
                ActivityKind::ToolUse => (theme.muted_foreground, "T"),
                ActivityKind::FileModified => (theme.accent, "F"),
                ActivityKind::SessionEvent => (theme.primary, "S"),
                ActivityKind::TaskEvent => (theme.success, "C"),
            };

            let is_file = matches!(entry.kind, ActivityKind::FileModified);

            let mut row = div()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(8.))
                .px(px(6.))
                .py(px(4.))
                .rounded(px(4.))
                .hover(|s| s.bg(theme.list_hover));

            if is_file {
                row = row.bg(theme.accent.opacity(0.06));
            }

            // Type indicator dot
            row = row.child(
                div()
                    .mt(px(4.))
                    .w(px(16.))
                    .h(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .bg(dot_color.opacity(0.12))
                    .text_color(dot_color)
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .child(icon_char),
            );

            let mut content = div().flex().flex_col().flex_grow().min_w_0();

            content = content.child(
                div()
                    .text_color(theme.sidebar_foreground)
                    .text_size(px(11.))
                    .overflow_x_hidden()
                    .child(entry.summary.clone()),
            );

            if let Some(path) = &entry.file_path {
                // Show only the filename for compact display
                let display_path = path.rsplit('/').next().unwrap_or(path);
                content = content.child(
                    div()
                        .text_color(theme.accent)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(10.))
                        .overflow_x_hidden()
                        .child(display_path.to_string()),
                );
            }

            row = row.child(content);
            list = list.child(row);
        }

        root.child(list)
    }
}
