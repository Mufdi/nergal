use gpui::*;
use gpui_component::ActiveTheme as _;

use crate::claude::cost::CostSummary;
use crate::hooks::events::HookEvent;

pub struct StatusBar {
    pub mode: String,
    pub session_info: String,
    pub cost: Option<CostSummary>,
}

impl StatusBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            mode: "idle".into(),
            session_info: "No active session".into(),
            cost: None,
        }
    }

    /// Update cost display from a parsed transcript.
    pub fn update_cost(&mut self, cost: CostSummary, cx: &mut Context<Self>) {
        self.cost = Some(cost);
        cx.notify();
    }

    pub fn handle_hook_event(&mut self, event: &HookEvent, cx: &mut Context<Self>) {
        match event {
            HookEvent::SessionStart { session_id } => {
                self.mode = "active".into();
                self.session_info = format!("Session: {}", truncate_id(session_id));
            }
            HookEvent::PreToolUse { tool_name, .. } => {
                self.mode = format!("tool: {tool_name}");
            }
            HookEvent::PostToolUse { .. } | HookEvent::UserPromptSubmit { .. } => {
                self.mode = "active".into();
            }
            HookEvent::Stop { .. } | HookEvent::SessionEnd { .. } => {
                self.mode = "idle".into();
                self.session_info = "No active session".into();
                self.cost = None;
            }
            HookEvent::TaskCompleted { .. } => {
                self.mode = "idle".into();
            }
        }
        cx.notify();
    }
}

fn truncate_id(id: &str) -> &str {
    let end = id.len().min(10);
    &id[..end]
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(28.))
            .px(px(12.))
            .bg(theme.secondary)
            .child(
                div()
                    .text_color(theme.success)
                    .child(format!("mode: {}", self.mode)),
            );

        if let Some(cost) = &self.cost {
            bar = bar.child(div().text_color(theme.warning).child(cost.display()));
        }

        bar.child(
            div()
                .text_color(theme.muted_foreground)
                .child(self.session_info.clone()),
        )
    }
}
