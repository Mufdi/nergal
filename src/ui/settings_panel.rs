use gpui::*;
use gpui_component::ActiveTheme as _;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};

use crate::config::Config;

/// Settings panel rendered inside a Sheet.
pub struct SettingsPanel {
    config: Config,
    claude_binary: Entity<InputState>,
    plans_dir: Entity<InputState>,
    transcripts_dir: Entity<InputState>,
    default_shell: Entity<InputState>,
    theme_mode: Entity<InputState>,
}

impl SettingsPanel {
    pub fn new(config: Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let claude_binary = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(config.claude_binary.clone(), window, cx);
            s
        });
        let plans_dir = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(config.plans_directory.display().to_string(), window, cx);
            s
        });
        let transcripts_dir = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(
                config.transcripts_directory.display().to_string(),
                window,
                cx,
            );
            s
        });
        let default_shell = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(config.default_shell.clone(), window, cx);
            s
        });
        let theme_mode = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(config.theme_mode.clone(), window, cx);
            s
        });

        Self {
            config,
            claude_binary,
            plans_dir,
            transcripts_dir,
            default_shell,
            theme_mode,
        }
    }

    fn save(&mut self, cx: &App) {
        self.config.claude_binary = self.claude_binary.read(cx).value().to_string();
        self.config.plans_directory = self.plans_dir.read(cx).value().to_string().into();
        self.config.transcripts_directory =
            self.transcripts_dir.read(cx).value().to_string().into();
        self.config.default_shell = self.default_shell.read(cx).value().to_string();
        self.config.theme_mode = self.theme_mode.read(cx).value().to_string();

        if let Err(e) = self.config.save() {
            tracing::error!("failed to save config: {e:#}");
        } else {
            tracing::info!("config saved");
        }
    }

    fn render_field(label: &str, input: &Entity<InputState>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .mb(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child(label.to_string()),
            )
            .child(Input::new(input))
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(16.))
            .bg(theme.background)
            .child(Self::render_field("Claude Binary", &self.claude_binary))
            .child(Self::render_field("Plans Directory", &self.plans_dir))
            .child(Self::render_field(
                "Transcripts Directory",
                &self.transcripts_dir,
            ))
            .child(Self::render_field("Default Shell", &self.default_shell))
            .child(Self::render_field("Theme (dark/light)", &self.theme_mode))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .mt(px(8.))
                    .child(
                        Button::new("save-settings")
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(cx);
                                window.close_sheet(cx);
                            })),
                    )
                    .child(
                        Button::new("cancel-settings")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|_, _, window, cx| {
                                window.close_sheet(cx);
                            })),
                    ),
            )
    }
}
