use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{Icon, IconName};

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

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .child(
                Icon::new(IconName::CircleCheck)
                    .size_4()
                    .text_color(theme.accent),
            )
            .child(
                div()
                    .text_color(theme.foreground)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(12.))
                    .child("TASKS"),
            )
            .when(self.store.visible_count() > 0, |this| {
                this.child(
                    div()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(8.))
                        .bg(theme.accent.opacity(0.15))
                        .text_color(theme.accent)
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{}", self.store.visible_count())),
                )
            });

        let root = div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .mb(px(2.))
            .bg(theme.sidebar)
            .rounded(theme.radius)
            .overflow_hidden()
            .child(header);

        if self.store.visible_count() == 0 {
            return root.child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .child(
                        Icon::new(IconName::CircleCheck)
                            .size_8()
                            .text_color(theme.muted_foreground.opacity(0.3)),
                    )
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .text_size(px(12.))
                            .child("No tasks yet"),
                    ),
            );
        }

        let mut list = div()
            .flex()
            .flex_col()
            .flex_grow()
            .overflow_y_scrollbar()
            .px(px(8.))
            .gap(px(2.));

        for task in self.store.visible_tasks() {
            let id = task.id.clone();
            let is_selected = self.selected.as_deref() == Some(&id);

            let (status_icon, status_color) = match task.status {
                TaskStatus::Pending => ("○", theme.muted_foreground),
                TaskStatus::InProgress => ("●", theme.accent),
                TaskStatus::Completed => ("✓", theme.success),
                TaskStatus::Deleted => continue,
            };

            let mut label_text = task.subject.clone();
            if task.status == TaskStatus::InProgress
                && let Some(ref form) = task.active_form
            {
                label_text = form.clone();
            }

            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(6.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme.list_hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.select_task(id.clone(), cx);
                    }),
                );

            if is_selected {
                row = row.bg(theme.list_active);
            }

            // Status dot
            row = row.child(
                div()
                    .text_color(status_color)
                    .text_size(px(10.))
                    .flex_shrink_0()
                    .child(status_icon),
            );

            let mut label = div()
                .flex_grow()
                .min_w_0()
                .overflow_x_hidden()
                .text_color(theme.sidebar_foreground)
                .text_size(px(12.))
                .child(label_text);

            if task.status == TaskStatus::Completed {
                label = label.line_through().text_color(theme.muted_foreground);
            }

            row = row.child(label);

            if !task.blocked_by.is_empty() {
                row = row.child(
                    div()
                        .text_color(theme.warning.opacity(0.7))
                        .text_size(px(9.))
                        .flex_shrink_0()
                        .child("blocked"),
                );
            }

            list = list.child(row);

            if is_selected && !task.description.is_empty() {
                list = list.child(
                    div()
                        .ml(px(26.))
                        .mb(px(4.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .bg(theme.secondary)
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .text_size(px(11.))
                                .child(task.description.clone()),
                        ),
                );
            }
        }

        root.child(list)
    }
}
