use gpui::*;
use gpui_component::sidebar::{Sidebar, SidebarMenu, SidebarMenuItem, SidebarToggleButton};
use gpui_component::{ActiveTheme as _, Icon, IconName};

/// Entry representing a session in the nav sidebar.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub label: String,
    pub active: bool,
}

/// Event emitted when the user interacts with the sidebar.
#[derive(Debug, Clone)]
pub enum NavSidebarEvent {
    SwitchSession(usize),
    ToggleCollapse,
}

impl EventEmitter<NavSidebarEvent> for NavSidebar {}

/// Left navigation sidebar with session list and collapse toggle.
pub struct NavSidebar {
    pub collapsed: bool,
    sessions: Vec<SessionEntry>,
    focus_handle: FocusHandle,
}

impl NavSidebar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            collapsed: false,
            sessions: Vec::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Update the session list displayed in the sidebar.
    pub fn set_sessions(&mut self, sessions: Vec<SessionEntry>, cx: &mut Context<Self>) {
        self.sessions = sessions;
        cx.notify();
    }

    /// Toggle collapsed state.
    pub fn toggle_collapse(&mut self, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.emit(NavSidebarEvent::ToggleCollapse);
        cx.notify();
    }
}

impl Render for NavSidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.collapsed;
        let theme = cx.theme();

        let mut menu = SidebarMenu::new();
        for (i, session) in self.sessions.iter().enumerate() {
            let idx = i;
            let item = SidebarMenuItem::new(SharedString::from(session.label.clone()))
                .icon(IconName::SquareTerminal)
                .active(session.active)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.emit(NavSidebarEvent::SwitchSession(idx));
                    let _ = this;
                }));
            menu = menu.child(item);
        }

        let toggle = SidebarToggleButton::left()
            .collapsed(collapsed)
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_collapse(cx);
            }));

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .child(Icon::new(IconName::Bot).size_4().text_color(theme.accent))
            .child(
                div()
                    .text_color(theme.accent)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(14.))
                    .child("cluihud"),
            );

        div().track_focus(&self.focus_handle).child(
            Sidebar::left()
                .collapsed(collapsed)
                .header(header)
                .child(menu)
                .footer(toggle)
                // Islands style: fill resizable slot, rounded, no border
                .w_full()
                .h_full()
                .mr(px(2.))
                .border_0()
                .rounded(theme.radius),
        )
    }
}
