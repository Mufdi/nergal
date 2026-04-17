use std::sync::atomic::{AtomicUsize, Ordering};
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;

/// cluihud's [`TerminalConfiguration`] implementation.
///
/// Differences from the wezterm-term defaults:
/// - Kitty keyboard protocol **on** by default so that Ctrl+Backspace,
///   Shift+Enter, Alt+letter, and friends encode distinctively. The user
///   can opt out via `terminal.kitty_keyboard = false` once Phase 3 wires
///   the config toggle.
/// - CSI-u as a fallback when Kitty is off (also distinctive, less capable).
/// - Scrollback defaults to 10_000 rows (wezterm default is 3_500; cluihud
///   sessions tend to produce longer tool-output transcripts).
#[derive(Debug)]
pub struct CluihudTerminalConfig {
    generation: AtomicUsize,
    kitty_keyboard: bool,
    scrollback_size: usize,
    color_palette: ColorPalette,
}

impl CluihudTerminalConfig {
    pub fn new() -> Self {
        Self {
            generation: AtomicUsize::new(0),
            kitty_keyboard: true,
            scrollback_size: 10_000,
            color_palette: ColorPalette::default(),
        }
    }

    pub fn with_kitty_keyboard(mut self, enabled: bool) -> Self {
        self.kitty_keyboard = enabled;
        self.generation.fetch_add(1, Ordering::Relaxed);
        self
    }

    pub fn with_scrollback(mut self, rows: usize) -> Self {
        self.scrollback_size = rows;
        self.generation.fetch_add(1, Ordering::Relaxed);
        self
    }
}

impl Default for CluihudTerminalConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalConfiguration for CluihudTerminalConfig {
    fn generation(&self) -> usize {
        self.generation.load(Ordering::Relaxed)
    }

    fn scrollback_size(&self) -> usize {
        self.scrollback_size
    }

    fn enable_csi_u_key_encoding(&self) -> bool {
        !self.kitty_keyboard
    }

    fn enable_kitty_keyboard(&self) -> bool {
        self.kitty_keyboard
    }

    fn color_palette(&self) -> ColorPalette {
        self.color_palette.clone()
    }
}
