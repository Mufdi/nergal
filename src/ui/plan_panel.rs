use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::Sizable as _;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::text::TextView;
use similar::{ChangeTag, TextDiff};

use crate::claude::plan::PlanManager;
use crate::hooks::state::HookState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanMode {
    View,
    Edit,
    Diff,
}

#[derive(Debug, Clone)]
pub enum PlanAction {
    AcceptClearContext,
    AcceptAutoEdits,
    AcceptManualEdits,
    Feedback(String),
}

impl EventEmitter<PlanAction> for PlanPanel {}

pub struct PlanPanel {
    mode: PlanMode,
    reviewing: bool,
    manager: PlanManager,
    editor_state: Option<Entity<InputState>>,
    feedback_state: Option<Entity<InputState>>,
    focus_handle: FocusHandle,
}

impl PlanPanel {
    pub fn new(plans_dir: std::path::PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            mode: PlanMode::View,
            reviewing: false,
            manager: PlanManager::new(plans_dir),
            editor_state: None,
            feedback_state: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Focus this panel.
    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    /// Called when a plan file change is detected — loads the latest plan for review.
    pub fn on_plan_ready(&mut self, cx: &mut Context<Self>) {
        match self.manager.find_latest_plan() {
            Ok(Some(path)) => {
                // Read file content to check if it actually changed
                let new_content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("failed to read plan: {e:#}");
                        return;
                    }
                };

                // Skip if this is the same content we already have (e.g. after our own save_edits)
                if let Some(current) = &self.manager.current_plan
                    && current.path == path
                    && current.content == new_content
                {
                    tracing::debug!("ignoring plan watcher event: content unchanged");
                    return;
                }

                if let Err(e) = self.manager.load_plan(&path) {
                    tracing::error!("failed to load plan: {e:#}");
                    return;
                }
                self.reviewing = true;
                self.mode = PlanMode::View;
                self.editor_state = None;
                tracing::info!("plan loaded for review: {}", path.display());
                cx.notify();
            }
            Ok(None) => {
                tracing::warn!("no plan files found in plans directory");
            }
            Err(e) => {
                tracing::error!("failed to find latest plan: {e:#}");
            }
        }
    }

    pub fn accept(&mut self, action: PlanAction, cx: &mut Context<Self>) {
        self.reviewing = false;
        self.mode = PlanMode::View;
        self.editor_state = None;
        self.feedback_state = None;
        cx.emit(action);
        cx.notify();
    }

    fn submit_feedback(&mut self, cx: &mut Context<Self>) {
        let text = self
            .feedback_state
            .as_ref()
            .map(|s| s.read(cx).value().to_string())
            .unwrap_or_default();

        if text.trim().is_empty() {
            return;
        }

        // Save edits if the plan was modified
        self.sync_editor_to_manager(cx);
        if self
            .manager
            .current_plan
            .as_ref()
            .is_some_and(|p| p.has_edits())
        {
            let content = self
                .manager
                .current_content()
                .unwrap_or_default()
                .to_string();

            match self.manager.save_edits(content) {
                Ok(path) => {
                    let state = HookState {
                        pending_plan_edit: Some(path.clone()),
                    };
                    if let Err(e) = state.write() {
                        tracing::error!("failed to write hook state: {e:#}");
                    }
                    tracing::info!("plan edits saved, hook state written: {}", path.display());
                }
                Err(e) => {
                    tracing::error!("failed to save plan edits: {e:#}");
                }
            }
        }

        self.accept(PlanAction::Feedback(text), cx);
    }

    fn submit_reread(&mut self, cx: &mut Context<Self>) {
        self.sync_editor_to_manager(cx);

        let content = self
            .manager
            .current_content()
            .unwrap_or_default()
            .to_string();

        match self.manager.save_edits(content) {
            Ok(path) => {
                let state = HookState {
                    pending_plan_edit: Some(path.clone()),
                };
                if let Err(e) = state.write() {
                    tracing::error!("failed to write hook state: {e:#}");
                }
                tracing::info!("plan edits saved for re-read: {}", path.display());
            }
            Err(e) => {
                tracing::error!("failed to save plan edits: {e:#}");
            }
        }

        self.accept(
            PlanAction::Feedback("Revise the plan based on my edits".to_string()),
            cx,
        );
    }

    fn enter_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self
            .manager
            .current_content()
            .unwrap_or_default()
            .to_string();
        let editor = cx.new(|cx| {
            let mut state = InputState::new(window, cx).multi_line(true);
            state.set_value(content, window, cx);
            state
        });
        self.editor_state = Some(editor);
        self.mode = PlanMode::Edit;
        cx.notify();
    }

    fn enter_view(&mut self, cx: &mut Context<Self>) {
        if self.mode == PlanMode::Edit {
            self.sync_editor_to_manager(cx);
        }
        self.editor_state = None;
        self.mode = PlanMode::View;
        cx.notify();
    }

    fn enter_diff(&mut self, cx: &mut Context<Self>) {
        if self.mode == PlanMode::Edit {
            self.sync_editor_to_manager(cx);
        }
        self.editor_state = None;
        self.mode = PlanMode::Diff;
        cx.notify();
    }

    fn sync_editor_to_manager(&mut self, cx: &App) {
        if let Some(editor) = &self.editor_state {
            let value = editor.read(cx).value().to_string();
            if let Some(plan) = self.manager.current_plan.as_mut() {
                plan.content = value;
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_content = self.manager.current_content().is_some();
        let has_edits = self
            .manager
            .current_plan
            .as_ref()
            .is_some_and(|p| p.has_edits());
        let theme = cx.theme();

        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(12.))
            .py(px(8.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_color(theme.accent)
                    .font_weight(FontWeight::BOLD)
                    .child("Plan"),
            );

        if has_content {
            let mut buttons = div().flex().flex_row().gap(px(4.));

            match self.mode {
                PlanMode::View => {
                    buttons = buttons.child(
                        Button::new("edit-btn")
                            .label("Edit")
                            .compact()
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.enter_edit(window, cx);
                            })),
                    );
                }
                PlanMode::Edit | PlanMode::Diff => {
                    buttons = buttons.child(
                        Button::new("view-btn")
                            .label("View")
                            .compact()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.enter_view(cx);
                            })),
                    );
                }
            }

            if has_edits && self.mode != PlanMode::Diff {
                buttons = buttons.child(
                    Button::new("diff-btn")
                        .label("Diff")
                        .compact()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.enter_diff(cx);
                        })),
                );
            }

            header = header.child(buttons);
        }

        header
    }

    fn render_diff(&self, cx: &App) -> Div {
        let theme = cx.theme();

        let Some(plan) = &self.manager.current_plan else {
            return div().flex_grow().px(px(12.)).py(px(8.)).child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("No plan loaded."),
            );
        };

        let diff = TextDiff::from_lines(&plan.original, &plan.content);
        let mut lines = div().flex().flex_col().font_family("monospace");

        let fg_equal = theme.foreground;
        let fg_insert = theme.success;
        let fg_delete = theme.danger;
        let bg_insert = Hsla {
            a: 0.15,
            ..theme.success
        };
        let bg_delete = Hsla {
            a: 0.15,
            ..theme.danger
        };

        for change in diff.iter_all_changes() {
            let value = change.value().trim_end_matches('\n');
            let (prefix, text_color, bg_color) = match change.tag() {
                ChangeTag::Equal => ("  ", fg_equal, None),
                ChangeTag::Insert => ("+ ", fg_insert, Some(bg_insert)),
                ChangeTag::Delete => ("- ", fg_delete, Some(bg_delete)),
            };

            let mut line = div()
                .px(px(12.))
                .py(px(1.))
                .text_color(text_color)
                .text_size(px(13.))
                .child(format!("{prefix}{value}"));

            if let Some(bg) = bg_color {
                line = line.bg(bg);
            }

            lines = lines.child(line);
        }

        div().flex_grow().py(px(8.)).child(lines)
    }

    fn ensure_feedback_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.feedback_state.is_none() {
            let state =
                cx.new(|cx| InputState::new(window, cx).placeholder("Type feedback for Claude..."));
            self.feedback_state = Some(state);
        }
    }

    fn render_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_feedback_state(window, cx);

        let theme = cx.theme();

        let primary_style = ButtonCustomVariant::new(cx)
            .color(theme.success)
            .foreground(theme.background);

        let secondary_style = ButtonCustomVariant::new(cx)
            .color(Hsla {
                a: 0.7,
                ..theme.success
            })
            .foreground(theme.background);

        let buttons = div()
            .flex()
            .flex_row()
            .gap(px(6.))
            .child(
                Button::new("accept-clear")
                    .label("Accept (clear ctx)")
                    .compact()
                    .custom(primary_style)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.accept(PlanAction::AcceptClearContext, cx);
                    })),
            )
            .child(
                Button::new("accept-auto")
                    .label("Accept (auto edits)")
                    .compact()
                    .custom(secondary_style)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.accept(PlanAction::AcceptAutoEdits, cx);
                    })),
            )
            .child(
                Button::new("accept-manual")
                    .label("Accept (manual)")
                    .compact()
                    .custom(secondary_style)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.accept(PlanAction::AcceptManualEdits, cx);
                    })),
            );

        let has_edits = self
            .manager
            .current_plan
            .as_ref()
            .is_some_and(|p| p.has_edits());

        let mut feedback_row = div().flex().flex_row().gap(px(6.)).mt(px(6.));

        if has_edits {
            let reread_style = ButtonCustomVariant::new(cx)
                .color(theme.accent)
                .foreground(theme.background);

            feedback_row = feedback_row.child(
                Button::new("reread-plan")
                    .label("Re-read edited plan")
                    .compact()
                    .custom(reread_style)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.submit_reread(cx);
                    })),
            );
        }

        if let Some(input_state) = &self.feedback_state {
            feedback_row = feedback_row
                .child(div().flex_grow().child(Input::new(input_state).small()))
                .child(
                    Button::new("send-feedback")
                        .label("Send")
                        .compact()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.submit_feedback(cx);
                        })),
                );
        }

        div()
            .flex()
            .flex_col()
            .px(px(12.))
            .py(px(8.))
            .border_t_1()
            .border_color(theme.border)
            .child(buttons)
            .child(feedback_row)
    }
}

impl Render for PlanPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(cx));

        match self.mode {
            PlanMode::View => {
                let content = self
                    .manager
                    .current_content()
                    .unwrap_or("No plan loaded.")
                    .to_string();

                container = container.child(
                    div()
                        .flex_grow()
                        .overflow_y_scrollbar()
                        .px(px(12.))
                        .py(px(8.))
                        .child(
                            TextView::markdown("plan-markdown", content, window, cx)
                                .selectable(true),
                        ),
                );
            }
            PlanMode::Edit => {
                if let Some(editor) = &self.editor_state {
                    container = container.child(
                        div()
                            .flex_grow()
                            .px(px(8.))
                            .py(px(4.))
                            .child(Input::new(editor).h_full()),
                    );
                }
            }
            PlanMode::Diff => {
                container = container.child(self.render_diff(cx));
            }
        }

        if self.reviewing {
            container = container.child(self.render_actions(window, cx));
        }

        container
    }
}
