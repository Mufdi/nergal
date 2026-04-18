use std::sync::atomic::{AtomicUsize, Ordering};
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;

/// cluihud's [`TerminalConfiguration`] implementation.
///
/// Differences from the wezterm-term defaults:
/// - **CSI-u key encoding off** (same as wezterm default). We previously
///   turned it on globally hoping to disambiguate Ctrl+Backspace, but that
///   wraps the byte in `\x1b[8;5u` which breaks the shells' default
///   `^H → backward-kill-word` binding. Leaving CSI-u off means
///   Ctrl+Backspace emits plain `\x08` (BS) and Backspace emits `\x7f`
///   (DEL) — same as Ghostty, and every mainstream shell already binds
///   them correctly.
/// - **Kitty keyboard protocol on by default**. Applications can still
///   opt into the richer protocol via `CSI > 1 u`; Kitty supersedes CSI-u
///   when the running app enables it.
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
        // Default off so Ctrl+Backspace emits plain `\x08` (BS) and shells
        // with the default `^H → backward-kill-word` binding Just Work.
        // When the user disables Kitty, we flip CSI-u on instead so keys
        // stay disambiguated via the CSI-u fallback.
        !self.kitty_keyboard
    }

    fn enable_kitty_keyboard(&self) -> bool {
        self.kitty_keyboard
    }

    fn color_palette(&self) -> ColorPalette {
        self.color_palette.clone()
    }
}
