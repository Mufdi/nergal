use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use termwiz::color::SrgbaTuple;
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;

pub type DefaultColors = ([u8; 3], [u8; 3]);
pub type ColorCell = Arc<RwLock<Option<DefaultColors>>>;

static SHARED_COLORS: LazyLock<ColorCell> = LazyLock::new(|| Arc::new(RwLock::new(None)));

pub fn shared_colors() -> ColorCell {
    Arc::clone(&SHARED_COLORS)
}

pub fn set_colors(cell: &ColorCell, fg: [u8; 3], bg: [u8; 3]) {
    if let Ok(mut slot) = cell.write() {
        *slot = Some((fg, bg));
    }
}

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
    colors: ColorCell,
}

impl NergalTerminalConfig {
    pub fn new() -> Self {
        Self::with_colors(shared_colors())
    }

    pub fn with_colors(colors: ColorCell) -> Self {
        Self {
            generation: AtomicUsize::new(0),
            scrollback_size: 10_000,
            color_palette: ColorPalette::default(),
            colors,
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

    fn enable_kitty_graphics(&self) -> bool {
        true
    }

    fn color_palette(&self) -> ColorPalette {
        let mut palette = self.color_palette.clone();
        if let Some((fg, bg)) = self.colors.read().ok().and_then(|slot| *slot) {
            palette.foreground = SrgbaTuple::from((fg[0], fg[1], fg[2]));
            palette.background = SrgbaTuple::from((bg[0], bg[1], bg[2]));
        }
        palette
    }
}
