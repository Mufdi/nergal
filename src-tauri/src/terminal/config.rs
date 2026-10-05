use std::sync::atomic::{AtomicUsize, Ordering};
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;

/// nergal's [`TerminalConfiguration`] implementation.
///
/// Differences from the wezterm-term defaults:
/// - **CSI-u key encoding off** (same as wezterm default). Turning it on
///   wraps Ctrl+Backspace in `\x1b[8;5u`, which breaks the shells' default
///   `^H -> backward-kill-word` binding.
/// - **Kitty keyboard protocol always off**. termwiz never encodes
///   kitty keys, so answering `CSI ? u` makes apps believe kitty is active
///   and skip their modifyOtherKeys request, leaving Ctrl+Enter as `\r`.
///   Staying silent makes Claude Code fall back to modifyOtherKeys, which
///   termwiz does encode (`CSI 27;5;13~`).
/// - Scrollback defaults to 10_000 rows (wezterm default is 3_500; nergal
///   sessions tend to produce longer tool-output transcripts).
#[derive(Debug)]
pub struct NergalTerminalConfig {
    generation: AtomicUsize,
    scrollback_size: usize,
    color_palette: ColorPalette,
}

impl NergalTerminalConfig {
    pub fn new() -> Self {
        Self {
            generation: AtomicUsize::new(0),
            scrollback_size: 10_000,
            color_palette: ColorPalette::default(),
        }
    }

    pub fn with_scrollback(mut self, rows: usize) -> Self {
        self.scrollback_size = rows;
        self.generation.fetch_add(1, Ordering::Relaxed);
        self
    }
}

impl Default for NergalTerminalConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalConfiguration for NergalTerminalConfig {
    fn generation(&self) -> usize {
        self.generation.load(Ordering::Relaxed)
    }

    fn scrollback_size(&self) -> usize {
        self.scrollback_size
    }

    fn enable_csi_u_key_encoding(&self) -> bool {
        false
    }

    fn enable_kitty_keyboard(&self) -> bool {
        false
    }

    fn color_palette(&self) -> ColorPalette {
        self.color_palette.clone()
    }
}
