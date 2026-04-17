use std::sync::atomic::{AtomicUsize, Ordering};
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;

/// cluihud's [`TerminalConfiguration`] implementation.
///
/// Differences from the wezterm-term defaults:
/// - **CSI-u key encoding always on**. This makes Ctrl+Backspace,
///   Shift+Enter, Alt+letter, and friends encode with unambiguous
///   sequences without requiring the running shell to opt into anything.
///   It is the lower-level knob that most directly gives us the "like
///   Ghostty" keyboard behavior.
/// - **Kitty keyboard protocol on by default**. Applications that want the
///   richer protocol (typing-event granularity, modifier reports) can opt
///   in via `CSI > 1 u`; if Kitty is off we still get CSI-u.
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
        // Always on: gives us Ctrl+Backspace ≠ Backspace, Shift+Enter ≠
        // Enter, Alt+letter with explicit encoding, regardless of whether
        // the running application ever opts into Kitty.
        true
    }

    fn enable_kitty_keyboard(&self) -> bool {
        self.kitty_keyboard
    }

    fn color_palette(&self) -> ColorPalette {
        self.color_palette.clone()
    }
}
