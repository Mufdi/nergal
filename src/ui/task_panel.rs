use gpui::*;
use gpui_component::ActiveTheme as _;

use crate::tasks::{TaskStatus, TaskStore};

pub struct TaskPanel {
    store: TaskStore,
    selected: Option<String>,
    focus_handle: FocusHandle,
}

impl TaskPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            store: TaskStore::new(),
            selected: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus this panel.
    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    /// Handle a PostToolUse event for TaskCreate or TaskUpdate.
    pub fn handle_tool_event(
        &mut self,
        tool_name: &str,
        tool_input: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        match tool_name {
            "TaskCreate" => {
                let id = self.store.apply_create(tool_input);
                tracing::info!("TaskCreate applied, assigned id={id:?}");
            }
            "TaskUpdate" => {
                let task_id = tool_input
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let status = tool_input
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let found = self.store.get(task_id).is_some();
                tracing::info!(
                    "TaskUpdate taskId={task_id} status={status} found_in_store={found}"
                );
                self.store.apply_update(tool_input);
            }
            _ => return,
        }
        cx.notify();
    }

    /// Replace the entire store (used by transcript re-parse).
    pub fn replace_store(&mut self, store: TaskStore, cx: &mut Context<Self>) {
        tracing::info!(
            "replace_store: {} visible tasks from transcript",
            store.visible_count()
        );
        self.store.replace_all(store);
        cx.notify();
    }

    fn select_task(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(&id) {
            self.selected = None;
        } else {
            self.selected = Some(id);
        }
        cx.notify();
    }
}

impl Render for TaskPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let mut root = div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .p(px(8.));

        root = root.child(
            div()
                .text_color(theme.accent)
                .font_weight(FontWeight::BOLD)
                .mb(px(8.))
                .child("Tasks"),
        );

        if self.store.visible_count() == 0 {
            return root.child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("No tasks yet."),
            );
        }

        for task in self.store.visible_tasks() {
            let id = task.id.clone();
            let is_selected = self.selected.as_deref() == Some(&id);

            let (status_icon, status_color) = match task.status {
                TaskStatus::Pending => ("○", theme.warning),
                TaskStatus::InProgress => ("◉", theme.primary),
                TaskStatus::Completed => ("✓", theme.success),
                TaskStatus::Deleted => continue,
            };

            let mut label_text = format!("{status_icon} {}", task.subject);
            if task.status == TaskStatus::InProgress
                && let Some(ref form) = task.active_form
            {
                label_text = format!("{status_icon} {form}");
            }

            let mut row = div()
                .px(px(6.))
                .py(px(4.))
                .rounded(px(4.))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.select_task(id.clone(), cx);
                    }),
                );

            if is_selected {
                row = row.bg(theme.secondary);
            }

            let mut label = div().text_color(status_color).child(label_text);
            if task.status == TaskStatus::Completed {
                label = label.line_through();
            }

            row = row.child(label);

            if !task.blocked_by.is_empty() {
                row = row.child(
                    div()
                        .text_color(theme.muted_foreground)
                        .text_size(px(11.))
                        .child(format!("blocked by: {}", task.blocked_by.join(", "))),
                );
            }

            root = root.child(row);

            if is_selected && !task.description.is_empty() {
                root = root.child(
                    div()
                        .mx(px(12.))
                        .mb(px(4.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(4.))
                        .bg(theme.secondary)
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .text_size(px(12.))
                                .child(task.description.clone()),
                        ),
                );
            }
        }

        root
    }
}
