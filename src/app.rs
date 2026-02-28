use gpui::*;

use crate::config::Config;
use crate::workspace::Workspace;

pub struct AppState {
    pub workspace: Entity<Workspace>,
}

impl AppState {
    pub fn new(config: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            workspace: cx.new(|cx| Workspace::new(config, window, cx)),
        }
    }
}

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.workspace.clone())
    }
}
