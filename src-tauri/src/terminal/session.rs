use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;
use wezterm_term::{Terminal, TerminalSize};

use super::config::NergalTerminalConfig;
use super::image_fetch::{ImageRegistry, hash_to_hex};
use super::input::map_event;
use super::program_status::{Osc7501, ProgramStatusScanner};
use super::types::{CellSnapshot, CursorSnapshot, GridSnapshot, ImageRun, TerminalKeyEvent};

pub type SyncState = Arc<Mutex<Option<Instant>>>;

const SYNC_ON: &[u8] = b"\x1b[?2026h";
const SYNC_OFF: &[u8] = b"\x1b[?2026l";
const FULL_RESET: &[u8] = b"\x1bc";

enum SyncEvent {
    On,
    Off,
    Reset,
}

fn match_sync(window: &[u8]) -> Option<SyncEvent> {
    if window.ends_with(SYNC_ON) {
        Some(SyncEvent::On)
    } else if window.ends_with(SYNC_OFF) {
        Some(SyncEvent::Off)
    } else if window.ends_with(FULL_RESET) {
        Some(SyncEvent::Reset)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PrimaryButton {
    Left,
    Middle,
    Right,
    None,
}

#[derive(Debug, Clone, Copy)]
pub enum PrimaryKind {
    Press,
    Release,
    Move,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

const FALLBACK_CELL_WIDTH: usize = 8;
const FALLBACK_CELL_HEIGHT: usize = 16;

/// wezterm-term divides by `pixel / cols` while placing inline images, so an axis below one
/// pixel per cell must never reach it.
pub fn clamp_pixel_geometry(
    cols: u16,
    rows: u16,
    pixel_width: Option<u16>,
    pixel_height: Option<u16>,
) -> (u16, u16) {
    fn axis(count: u16, pixels: Option<u16>, fallback_cell: usize) -> u16 {
        let count = usize::from(count);
        let value = match pixels {
            Some(p) if usize::from(p) >= count => usize::from(p),
            _ => count * fallback_cell,
        };
        value.min(usize::from(u16::MAX)) as u16
    }
    (
        axis(cols, pixel_width, FALLBACK_CELL_WIDTH),
        axis(rows, pixel_height, FALLBACK_CELL_HEIGHT),
    )
}

/// A single terminal emulator instance owned by the backend.
///
/// Phase 1 only: no IPC coupling, no coalescing, no grid differ. Those land
/// in Phase 2. What this gives us is the ability to feed PTY bytes in, read
/// the resulting screen state out, and cover it with unit tests.
pub struct TerminalSession {
    terminal: Terminal,
    cols: u16,
    rows: u16,
    pixel_width: u16,
    pixel_height: u16,
    /// Snapshot of the palette at construction time. Used to resolve
    /// `PaletteIndex(n)` cell attributes into concrete RGBA when building
    /// a [`GridSnapshot`] — without this, the frontend would see `None`
    /// for every ANSI-colored character and render the default foreground.
    palette: ColorPalette,
    /// How many lines above the live bottom the visible window is currently
    /// scrolled. `0` means pinned to the bottom; positive values reveal
    /// scrollback history.
    scroll_offset: usize,
    /// `screen.scrollback_rows()` observed during the previous
    /// `grid_snapshot` call. Used to keep the user's view anchored on the
    /// same content when new PTY output pushes lines into scrollback while
    /// the user is scrolled up.
    last_scrollback_rows: usize,
    sync_since: SyncState,
    sync_tail: Vec<u8>,
    image_registry: ImageRegistry,
    status_scanner: ProgramStatusScanner,
    status_reply: Option<Box<dyn Write + Send>>,
}

impl TerminalSession {
    /// Build a session. `writer` is whatever the terminal should use to talk
    /// back to the PTY (answerbacks, mouse reports, encoded keyboard input).
    /// In tests pass a `Vec<u8>` or `std::io::sink()`; in production this
    /// will be wired to the `portable-pty` master writer.
    pub fn new(cols: u16, rows: u16, writer: Box<dyn Write + Send>) -> Self {
        Self::with_config(cols, rows, writer, NergalTerminalConfig::new())
    }

    pub fn new_with_pixels(
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        Self::with_config_and_pixels(
            cols,
            rows,
            pixel_width,
            pixel_height,
            writer,
            NergalTerminalConfig::new(),
        )
    }

    pub fn with_config(
        cols: u16,
        rows: u16,
        writer: Box<dyn Write + Send>,
        config: NergalTerminalConfig,
    ) -> Self {
        Self::with_config_and_pixels(cols, rows, None, None, writer, config)
    }

    pub fn with_config_and_pixels(
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
        writer: Box<dyn Write + Send>,
        config: NergalTerminalConfig,
    ) -> Self {
        let (pixel_width, pixel_height) =
            clamp_pixel_geometry(cols, rows, pixel_width, pixel_height);
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: pixel_width as usize,
            pixel_height: pixel_height as usize,
            dpi: 0,
        };
        let palette = config.color_palette();
        let terminal = Terminal::new(
            size,
            Arc::new(config),
            "nergal",
            env!("CARGO_PKG_VERSION"),
            writer,
        );
        Self {
            terminal,
            cols,
            rows,
            pixel_width,
            pixel_height,
            palette,
            scroll_offset: 0,
            last_scrollback_rows: 0,
            sync_since: Arc::new(Mutex::new(None)),
            sync_tail: Vec::new(),
            image_registry: ImageRegistry::default(),
            status_scanner: ProgramStatusScanner::default(),
            status_reply: None,
        }
    }

    pub fn set_status_reply(&mut self, writer: Box<dyn Write + Send>) {
        self.status_reply = Some(writer);
    }

    pub fn image_registry(&self) -> &ImageRegistry {
        &self.image_registry
    }

    pub fn sync_output_since(&self) -> Option<Instant> {
        self.sync_since.lock().ok().and_then(|slot| *slot)
    }

    pub fn sync_state(&self) -> SyncState {
        Arc::clone(&self.sync_since)
    }

    // wezterm-term tracks no 2026 state of its own, and sequences can straddle PTY reads,
    // so the last 7 bytes are carried between calls.
    fn track_sync_mode(&mut self, bytes: &[u8]) {
        let carried = self.sync_tail.len();
        let mut window = std::mem::take(&mut self.sync_tail);
        window.extend_from_slice(bytes);
        for end in 1..=window.len() {
            // Matches that finish inside the carried tail were already handled.
            if end <= carried {
                continue;
            }
            let Some(event) = match_sync(&window[..end]) else {
                continue;
            };
            let Ok(mut slot) = self.sync_since.lock() else {
                continue;
            };
            match event {
                SyncEvent::On => {
                    if slot.is_none() {
                        *slot = Some(Instant::now());
                    }
                }
                SyncEvent::Off | SyncEvent::Reset => *slot = None,
            }
        }
        let keep = window.len().min(SYNC_ON.len() - 1);
        self.sync_tail = window.split_off(window.len() - keep);
    }

    /// Feed bytes from the PTY reader into the VT parser. Returns the OSC 7501 report bodies
    /// found in this chunk; queries are answered here, before wezterm parses the chunk, so the
    /// reply is queued ahead of wezterm's own answer to a DA1 that follows in the same read.
    pub fn advance_bytes(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.track_sync_mode(bytes);
        let mut reports = Vec::new();
        for found in self.status_scanner.feed(bytes) {
            match found {
                Osc7501::Query => {
                    if let Some(reply) = self.status_reply.as_mut()
                        && let Err(e) = reply.write_all(b"\x1b]7501;?\x1b\\")
                    {
                        tracing::warn!("OSC 7501 reply failed: {e}");
                    }
                }
                Osc7501::Report(body) => reports.push(body),
            }
        }
        self.terminal.advance_bytes(bytes);
        reports
    }

    /// The PTY resize (ioctl TIOCSWINSZ) stays on the `pty.rs` side; this only updates the
    /// emulator's internal model.
    ///
    /// Returns `false` without touching the emulator when the clamped size is unchanged:
    /// wezterm's resize always resets the scroll margins and cursor, so it is not idempotent.
    pub fn resize_with_pixels(
        &mut self,
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
    ) -> bool {
        if !self.size_changed(cols, rows, pixel_width, pixel_height) {
            return false;
        }
        self.apply_resize(cols, rows, pixel_width, pixel_height);
        true
    }

    pub fn size_changed(
        &self,
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
    ) -> bool {
        let (pw, ph) = clamp_pixel_geometry(cols, rows, pixel_width, pixel_height);
        (cols, rows, pw, ph) != (self.cols, self.rows, self.pixel_width, self.pixel_height)
    }

    fn apply_resize(
        &mut self,
        cols: u16,
        rows: u16,
        pixel_width: Option<u16>,
        pixel_height: Option<u16>,
    ) {
        let (pixel_width, pixel_height) =
            clamp_pixel_geometry(cols, rows, pixel_width, pixel_height);
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: pixel_width as usize,
            pixel_height: pixel_height as usize,
            dpi: 0,
        };
        self.terminal.resize(size);
        self.cols = cols;
        self.rows = rows;
        self.pixel_width = pixel_width;
        self.pixel_height = pixel_height;
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Translate and encode a frontend key event. wezterm owns the actual
    /// byte encoding (CSI-u, Kitty keyboard protocol, etc.) and writes the
    /// result into the writer registered at construction time.
    ///
    /// Returns `Ok(false)` when the event carries no mappable key (e.g. a
    /// dead key stroke or a browser-only synthetic event); callers can
    /// treat that as a silent no-op.
    pub fn key_down(&mut self, event: &TerminalKeyEvent) -> Result<bool> {
        let Some((key, mods)) = map_event(event) else {
            return Ok(false);
        };
        self.terminal.key_down(key, mods)?;
        Ok(true)
    }

    /// Safe to call on every DOM focus/blur: wezterm emits `CSI I` / `CSI O`
    /// only when the app enabled DECSET 1004, and drops repeats.
    pub fn focus_changed(&mut self, focused: bool) {
        self.terminal.focus_changed(focused);
    }

    /// Read-only access to the underlying wezterm terminal. Useful for tests
    /// and for Phase 2 grid-diffing logic that needs richer access than
    /// [`Self::grid_snapshot`] exposes.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Cursor column on the live screen.
    pub fn cursor_col(&self) -> usize {
        self.terminal.cursor_pos().x
    }

    /// Plain text of the live-screen row the cursor sits on. Used by the
    /// aux-shell command tracker at Enter: the displayed line carries what
    /// keystroke mirroring can't see (history recall, tab completion).
    pub fn cursor_line_text(&self) -> String {
        let y = self.terminal.cursor_pos().y.max(0) as usize;
        let screen = self.terminal.screen();
        let start = screen
            .scrollback_rows()
            .saturating_sub(screen.physical_rows);
        let row = start + y;
        let lines = screen.lines_in_phys_range(row..row + 1);
        let Some(line) = lines.first() else {
            return String::new();
        };
        let mut text = String::new();
        for cell in line.visible_cells() {
            text.push_str(cell.str());
        }
        text.trim_end().to_string()
    }

    /// Current scroll position, in lines above the live bottom. `0` means the
    /// viewport is pinned to the live PTY output.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Maximum offset reachable right now: how many full extra screens of
    /// history are available above the visible window.
    pub fn max_scroll_offset(&self) -> usize {
        let screen = self.terminal.screen();
        screen
            .scrollback_rows()
            .saturating_sub(screen.physical_rows)
    }

    /// Set the scroll offset directly, clamped to `[0, max_scroll_offset]`.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max = self.max_scroll_offset();
        self.scroll_offset = offset.min(max);
    }

    /// Adjust the scroll offset by `delta` lines. Positive = up (more
    /// history); negative = down (toward live bottom). Returns the new
    /// offset after clamping.
    pub fn scroll_by(&mut self, delta: i32) -> usize {
        let max = self.max_scroll_offset();
        let current = self.scroll_offset as i64;
        let next = (current + delta as i64).clamp(0, max as i64);
        self.scroll_offset = next as usize;
        self.scroll_offset
    }

    /// Snap the viewport back to the live bottom.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.terminal.bracketed_paste_enabled()
    }

    /// True when the underlying terminal is rendering on the alternate
    /// screen (TUI apps like vim, less, claude in fullscreen). Local
    /// scrollback is unavailable there, so wheel events should be
    /// forwarded to the app via [`Self::mouse_wheel`] instead.
    pub fn is_alt_screen_active(&self) -> bool {
        self.terminal.is_alt_screen_active()
    }

    /// Forward a mouse button event to wezterm-term. wezterm emits the
    /// encoded mouse report when the running app has enabled a mouse mode
    /// (`?1000h`, `?1002h`, `?1003h`; `?1006h` only selects the SGR
    /// encoding); without an enabled mode this is a no-op. CC's TUI relies
    /// on this to detect clicks on interactive menus / lists; without
    /// forwarding the panel stays inert.
    pub fn mouse_button(
        &mut self,
        button: PrimaryButton,
        kind: PrimaryKind,
        col: u16,
        row: u16,
        mods: MouseMods,
    ) -> Result<()> {
        use wezterm_term::input::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        // The viewport shows history rows, so the app would receive a click on
        // a row it never drew there. Releases still go out: a button pressed at
        // the live bottom must not stay held in the app after a scroll.
        if matches!(kind, PrimaryKind::Press)
            && !self.terminal.is_alt_screen_active()
            && self.scroll_offset > 0
        {
            return Ok(());
        }
        let mut modifiers = KeyModifiers::default();
        if mods.shift {
            modifiers |= KeyModifiers::SHIFT;
        }
        if mods.ctrl {
            modifiers |= KeyModifiers::CTRL;
        }
        if mods.alt {
            modifiers |= KeyModifiers::ALT;
        }
        let wez_button = match button {
            PrimaryButton::Left => MouseButton::Left,
            PrimaryButton::Middle => MouseButton::Middle,
            PrimaryButton::Right => MouseButton::Right,
            PrimaryButton::None => MouseButton::None,
        };
        let wez_kind = match kind {
            PrimaryKind::Press => MouseEventKind::Press,
            PrimaryKind::Release => MouseEventKind::Release,
            PrimaryKind::Move => MouseEventKind::Move,
        };
        self.terminal.mouse_event(MouseEvent {
            kind: wez_kind,
            button: wez_button,
            x: col as usize,
            y: row as i64,
            x_pixel_offset: 0,
            y_pixel_offset: 0,
            modifiers,
        })
    }

    /// Hand a wheel tick to wezterm-term's mouse pipeline. wezterm picks
    /// the encoding based on what the running app requested:
    /// SGR mouse report, X10/UTF-8 mouse report, or — when no mouse mode
    /// is enabled but the alt screen is active — synthetic arrow-key
    /// presses (xterm's "alternateScroll" emulation). When neither
    /// applies it's a no-op.
    ///
    /// `delta_lines` follows the rest of the scroll API: positive = up,
    /// negative = down. `col`/`row` are cell coordinates of the cursor
    /// at the time of the wheel event; many apps ignore them but mouse
    /// reports need them for correctness.
    pub fn mouse_wheel(&mut self, delta_lines: i32, col: u16, row: u16) -> Result<()> {
        use wezterm_term::input::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let count = delta_lines.unsigned_abs() as usize;
        if count == 0 {
            return Ok(());
        }
        let button = if delta_lines > 0 {
            MouseButton::WheelUp(count)
        } else {
            MouseButton::WheelDown(count)
        };
        self.terminal.mouse_event(MouseEvent {
            kind: MouseEventKind::Press,
            button,
            x: col as usize,
            y: row as i64,
            x_pixel_offset: 0,
            y_pixel_offset: 0,
            modifiers: KeyModifiers::default(),
        })
    }

    /// Extract a renderable snapshot of the visible screen. Phase 2 replaces
    /// the ad-hoc "only changed rows" diffing on top of this. For Phase 1
    /// tests, the caller can compare two snapshots directly.
    pub fn grid_snapshot(&mut self) -> GridSnapshot {
        let (cols, total_rows, physical_rows) = {
            let screen = self.terminal.screen();
            (
                screen.physical_cols,
                screen.scrollback_rows(),
                screen.physical_rows,
            )
        };

        // Anchor: when the user is scrolled up and new content pushed
        // additional lines into scrollback, advance the offset by the same
        // amount so the visible content stays put under their eyes.
        if self.scroll_offset > 0 && total_rows > self.last_scrollback_rows {
            let growth = total_rows - self.last_scrollback_rows;
            let max = total_rows.saturating_sub(physical_rows);
            self.scroll_offset = (self.scroll_offset + growth).min(max);
        } else {
            let max = total_rows.saturating_sub(physical_rows);
            if self.scroll_offset > max {
                self.scroll_offset = max;
            }
        }
        self.last_scrollback_rows = total_rows;

        let end = total_rows.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(physical_rows);
        let screen = self.terminal.screen();
        let visible = screen.lines_in_phys_range(start..end);
        let mut rows: Vec<Vec<CellSnapshot>> = Vec::with_capacity(visible.len());
        let mut images: Vec<Vec<ImageRun>> = Vec::with_capacity(visible.len());
        self.image_registry.prune();
        let mut hex_by_image: HashMap<*const termwiz::image::ImageData, String> = HashMap::new();

        for line in &visible {
            let mut row_cells: Vec<CellSnapshot> = Vec::with_capacity(cols);
            let mut runs: Vec<ImageRun> = Vec::new();
            for cell in line.visible_cells() {
                let attrs = cell.attrs();
                if let Some(cell_images) = attrs.images() {
                    for image in &cell_images {
                        let data = image.image_data();
                        let hex = hex_by_image.entry(Arc::as_ptr(data)).or_insert_with(|| {
                            self.image_registry.insert(data);
                            hash_to_hex(&data.hash())
                        });
                        push_image_cell_hashed(&mut runs, cell.cell_index(), image, hex);
                    }
                }
                let snap = CellSnapshot {
                    ch: cell.str().to_owned(),
                    fg: color_to_rgba(attrs.foreground(), &self.palette),
                    bg: color_to_rgba(attrs.background(), &self.palette),
                    bold: attrs.intensity() == termwiz::cell::Intensity::Bold,
                    italic: attrs.italic(),
                    underline: attrs.underline() != termwiz::cell::Underline::None,
                    reverse: attrs.reverse(),
                    hyperlink: attrs.hyperlink().map(|h| h.uri().to_owned()),
                };
                row_cells.push(snap);
            }
            while row_cells.len() < cols {
                row_cells.push(CellSnapshot::default());
            }
            rows.push(row_cells);
            images.push(runs);
        }

        let cursor_pos = self.terminal.cursor_pos();
        // The cursor lives at the bottom of the live screen; when the user
        // scrolls back through history its physical row falls outside the
        // window. Hide it instead of rendering a phantom caret on the wrong
        // line.
        let cursor_visible = self.scroll_offset == 0
            && cursor_pos.visibility == termwiz::surface::CursorVisibility::Visible;
        let cursor = CursorSnapshot {
            x: cursor_pos.x,
            y: cursor_pos.y.max(0) as usize,
            visible: cursor_visible,
        };

        let raw_title = self.terminal.get_title();
        let title = if raw_title.is_empty() {
            None
        } else {
            Some(raw_title.to_owned())
        };

        let is_alt_screen = self.terminal.is_alt_screen_active();

        GridSnapshot {
            cols,
            rows,
            cursor,
            title,
            scroll_offset: self.scroll_offset,
            is_alt_screen,
            mouse_tracking: self.terminal.is_mouse_grabbed(),
            images,
        }
    }
}

#[cfg(test)]
fn push_image_cell(runs: &mut Vec<ImageRun>, col: usize, image: &termwiz::image::ImageCell) {
    push_image_cell_hashed(runs, col, image, &hash_to_hex(&image.image_data().hash()));
}

/// wezterm hands out slices of one placement with `x` fractions that chain
/// exactly, so comparing them with equality is safe.
fn push_image_cell_hashed(
    runs: &mut Vec<ImageRun>,
    col: usize,
    image: &termwiz::image::ImageCell,
    hash: &str,
) {
    let top_left = image.top_left();
    let bottom_right = image.bottom_right();
    let (u0, v0) = (top_left.x.into_inner(), top_left.y.into_inner());
    let (u1, v1) = (bottom_right.x.into_inner(), bottom_right.y.into_inner());
    let (pad_left, pad_top, pad_right, pad_bottom) = image.padding();
    let z = image.z_index();

    let continues = |run: &ImageRun| {
        run.hash == hash
            && run.z == z
            && run.col + run.span == col
            && run.u1 == u0
            && run.v0 == v0
            && run.v1 == v1
            && run.pad_left == pad_left
            && run.pad_top == pad_top
    };
    if let Some(run) = runs.iter_mut().rev().find(|run| continues(run)) {
        run.span += 1;
        run.u1 = u1;
        run.pad_right = pad_right;
        run.pad_bottom = pad_bottom;
        return;
    }
    runs.push(ImageRun {
        hash: hash.to_owned(),
        col,
        span: 1,
        u0,
        v0,
        u1,
        v1,
        pad_left,
        pad_top,
        pad_right,
        pad_bottom,
        z,
    });
}

fn color_to_rgba(color: termwiz::color::ColorAttribute, palette: &ColorPalette) -> Option<[u8; 4]> {
    use termwiz::color::ColorAttribute;
    match color {
        ColorAttribute::Default => None,
        ColorAttribute::TrueColorWithDefaultFallback(srgb)
        | ColorAttribute::TrueColorWithPaletteFallback(srgb, _) => Some(srgb_to_u8(srgb)),
        ColorAttribute::PaletteIndex(idx) => {
            let srgb = palette.colors.0[idx as usize];
            Some(srgb_to_u8(srgb))
        }
    }
}

fn srgb_to_u8(srgb: termwiz::color::SrgbaTuple) -> [u8; 4] {
    let (r, g, b, a) = srgb.to_tuple_rgba();
    [scale(r), scale(g), scale(b), scale(a)]
}

fn scale(f: f32) -> u8 {
    (f.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Clone, Default)]
    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    impl CapturedWriter {
        fn new() -> Self {
            Self::default()
        }
        fn bytes(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
        fn clear(&self) {
            self.0.lock().unwrap().clear();
        }
        /// wezterm wraps our writer behind a channel + background thread, so
        /// bytes do not arrive synchronously. Poll for at least one byte with
        /// a generous timeout; return whatever we have when time is up.
        fn drain(&self) -> Vec<u8> {
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let bytes = self.bytes();
                if !bytes.is_empty() {
                    return bytes;
                }
                if Instant::now() >= deadline {
                    return bytes;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    impl Write for CapturedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn session(cols: u16, rows: u16) -> TerminalSession {
        TerminalSession::new(cols, rows, Box::new(Vec::<u8>::new()))
    }

    fn session_with_writer(cols: u16, rows: u16) -> (TerminalSession, CapturedWriter) {
        let w = CapturedWriter::new();
        let session = TerminalSession::new(cols, rows, Box::new(w.clone()));
        (session, w)
    }

    fn evt(code: &str, key: &str) -> TerminalKeyEvent {
        TerminalKeyEvent {
            code: code.into(),
            key: key.into(),
            text: None,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    fn plain_row(snapshot: &GridSnapshot, row: usize) -> String {
        snapshot.rows[row]
            .iter()
            .map(|c| c.ch.as_str())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    #[test]
    fn plain_text_lands_on_first_row() {
        let mut s = session(20, 5);
        s.advance_bytes(b"hello world");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "hello world");
        assert_eq!(grid.cursor.x, 11);
        assert_eq!(grid.cursor.y, 0);
    }

    #[test]
    fn newline_moves_cursor_down() {
        let mut s = session(20, 5);
        s.advance_bytes(b"line1\r\nline2");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "line1");
        assert_eq!(plain_row(&grid, 1), "line2");
        assert_eq!(grid.cursor.y, 1);
    }

    #[test]
    fn sgr_red_sets_foreground() {
        let mut s = session(10, 3);
        // ESC[31m = red fg (palette index 1 on default xterm palette = 0xcc5555);
        // "X"; ESC[0m = reset.
        s.advance_bytes(b"\x1b[31mX\x1b[0m");
        let grid = s.grid_snapshot();
        let first = &grid.rows[0][0];
        assert_eq!(first.ch, "X");
        let fg = first.fg.expect("palette index must resolve to an RGBA");
        assert_eq!(&fg[0..3], &[0xcc, 0x55, 0x55], "should be xterm maroon");
    }

    #[test]
    fn truecolor_sgr_is_preserved() {
        let mut s = session(10, 3);
        // 24-bit SGR: ESC[38;2;10;20;30m — fg = #0A141E.
        s.advance_bytes(b"\x1b[38;2;10;20;30mZ\x1b[0m");
        let fg = s.grid_snapshot().rows[0][0]
            .fg
            .expect("truecolor must survive");
        assert_eq!(&fg[0..3], &[10, 20, 30]);
    }

    #[test]
    fn clear_screen_wipes_rows() {
        let mut s = session(10, 3);
        s.advance_bytes(b"dirty line");
        assert_eq!(plain_row(&s.grid_snapshot(), 0), "dirty line");
        // ESC[2J clears entire screen; ESC[H homes cursor.
        s.advance_bytes(b"\x1b[2J\x1b[H");
        let grid = s.grid_snapshot();
        assert_eq!(plain_row(&grid, 0), "");
        assert_eq!(grid.cursor.x, 0);
        assert_eq!(grid.cursor.y, 0);
    }

    #[test]
    fn osc_8_hyperlink_is_captured() {
        let mut s = session(40, 3);
        // OSC 8 ; ; URL ST  "text"  OSC 8 ; ; ST
        s.advance_bytes(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
        let grid = s.grid_snapshot();
        let first = &grid.rows[0][0];
        assert_eq!(first.ch, "l");
        assert_eq!(first.hyperlink.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn resize_updates_dimensions() {
        let mut s = session(80, 24);
        assert_eq!(s.cols(), 80);
        assert_eq!(s.rows(), 24);
        s.resize_with_pixels(120, 40, None, None);
        assert_eq!(s.cols(), 120);
        assert_eq!(s.rows(), 40);
        let grid = s.grid_snapshot();
        assert_eq!(grid.cols, 120);
        assert_eq!(grid.rows.len(), 40);
    }

    #[test]
    fn title_updates_via_osc_sequence() {
        let mut s = session(20, 3);
        // OSC 0 sets icon and window title
        s.advance_bytes(b"\x1b]0;nergal test\x07");
        let grid = s.grid_snapshot();
        assert_eq!(grid.title.as_deref(), Some("nergal test"));
    }

    #[test]
    fn ctrl_backspace_emits_ctrl_w_for_word_delete() {
        // The map_event layer remaps Ctrl+Backspace to Ctrl+W so it lands
        // on every shell's default `backward-kill-word` binding without
        // requiring user-side shell config. Wezterm's ctrl_mapping('w')
        // yields `\x17` (ETB).
        let (mut s, w) = session_with_writer(80, 24);

        s.key_down(&evt("Backspace", "Backspace")).unwrap();
        let plain = w.drain();
        assert_eq!(plain, b"\x7f", "plain Backspace emits DEL");
        w.clear();

        let mut ctrl_bs = evt("Backspace", "Backspace");
        ctrl_bs.ctrl = true;
        s.key_down(&ctrl_bs).unwrap();
        let with_ctrl = w.drain();
        assert_eq!(with_ctrl, b"\x17", "Ctrl+Backspace remaps to Ctrl+W (ETB)");
    }

    #[test]
    fn shift_enter_matches_plain_enter_by_default() {
        // Default config keeps CSI-u off so shells see standard `\r` for
        // both Enter and Shift+Enter. Applications that want a distinct
        // sequence opt into Kitty (shell-side, via `CSI > 1 u`). This
        // documents the "behave like Ghostty by default" contract.
        let (mut s, w) = session_with_writer(80, 24);

        s.key_down(&evt("Enter", "Enter")).unwrap();
        let plain = w.drain();
        w.clear();

        let mut shift_enter = evt("Enter", "Enter");
        shift_enter.shift = true;
        s.key_down(&shift_enter).unwrap();
        let with_shift = w.drain();

        assert_eq!(plain, with_shift, "default config: Shift+Enter == Enter");
    }

    #[test]
    fn plain_printable_is_written_verbatim() {
        let (mut s, w) = session_with_writer(80, 24);
        s.key_down(&evt("KeyA", "a")).unwrap();
        assert_eq!(w.drain(), b"a");
    }

    #[test]
    fn alt_letter_emits_escape_prefixed_sequence() {
        let (mut s, w) = session_with_writer(80, 24);
        let mut alt_a = evt("KeyA", "a");
        alt_a.alt = true;
        s.key_down(&alt_a).unwrap();
        let bytes = w.drain();
        // Alt+letter must not encode to bare "a" — historically ESC-prefixed
        // in xterm, CSI-u in Kitty mode; either way it is not `b"a"`.
        assert_ne!(bytes, b"a");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn scroll_offset_clamped_when_no_history_available() {
        let mut s = session(20, 5);
        s.advance_bytes(b"hello");
        // Nothing has been pushed into scrollback; max offset should be 0
        // and any scroll request must clamp.
        assert_eq!(s.scroll_by(10), 0);
        assert_eq!(s.scroll_offset(), 0);
        let grid = s.grid_snapshot();
        assert_eq!(grid.scroll_offset, 0);
        assert!(grid.cursor.visible, "no scroll → cursor visible");
    }

    #[test]
    fn scroll_back_reveals_history_pushed_off_top() {
        // Build a session with 3 visible rows, fill it past capacity so the
        // earliest lines move into scrollback, then verify scroll_by exposes
        // them.
        let mut s = session(20, 3);
        for n in 0..6u8 {
            s.advance_bytes(format!("line{n}\r\n").as_bytes());
        }
        // After 6 lines + cursor newline, the 3 latest lines fill the
        // visible area. Bottom-pinned snapshot should not contain "line0".
        let live = s.grid_snapshot();
        let live_text: String = live
            .rows
            .iter()
            .map(|r| plain_text(r))
            .collect::<Vec<_>>()
            .join("|");
        assert!(
            !live_text.contains("line0"),
            "live view excludes scrollback"
        );

        // Now scroll back enough to expose line0.
        s.scroll_by(10);
        assert!(s.scroll_offset() > 0, "scroll up should leave bottom");
        let history = s.grid_snapshot();
        let history_text: String = history
            .rows
            .iter()
            .map(|r| plain_text(r))
            .collect::<Vec<_>>()
            .join("|");
        assert!(
            history_text.contains("line0"),
            "scrolled view should reveal line0; got '{history_text}'"
        );
        assert!(
            !history.cursor.visible,
            "cursor must be hidden while scrolled back"
        );
        assert!(history.scroll_offset > 0);
    }

    #[test]
    fn anchor_keeps_view_stable_when_new_lines_arrive() {
        // While the user is reading history, new PTY output must not yank
        // the visible content out from under them.
        let mut s = session(20, 3);
        for n in 0..6u8 {
            s.advance_bytes(format!("line{n}\r\n").as_bytes());
        }
        s.scroll_by(2);
        let before = s.grid_snapshot();
        let before_text = plain_text(&before.rows[0]);

        // Simulate new output landing while the user is scrolled up.
        s.advance_bytes(b"newone\r\nnewtwo\r\n");
        let after = s.grid_snapshot();
        let after_text = plain_text(&after.rows[0]);

        assert_eq!(
            before_text, after_text,
            "anchor: scrolled view should remain pinned to the same row"
        );
    }

    #[test]
    fn scroll_to_bottom_resets_offset() {
        let mut s = session(20, 3);
        for n in 0..6u8 {
            s.advance_bytes(format!("line{n}\r\n").as_bytes());
        }
        s.scroll_by(5);
        assert!(s.scroll_offset() > 0);
        s.scroll_to_bottom();
        assert_eq!(s.scroll_offset(), 0);
        let grid = s.grid_snapshot();
        assert!(grid.cursor.visible, "back at bottom → cursor visible again");
    }

    fn answer_to(seq: &[u8]) -> Vec<u8> {
        let (mut s, w) = session_with_writer(80, 24);
        s.advance_bytes(seq);
        w.drain()
    }

    #[test]
    fn kitty_keyboard_query_is_not_answered_by_default() {
        assert!(answer_to(b"\x1b[?u").is_empty());
    }

    #[test]
    fn kitty_flags_push_does_not_change_ctrl_enter() {
        let (mut s, w) = session_with_writer(80, 24);
        s.advance_bytes(b"\x1b[>1u");
        let mut ctrl_enter = evt("Enter", "Enter");
        ctrl_enter.ctrl = true;
        s.key_down(&ctrl_enter).unwrap();
        assert_eq!(w.drain(), b"\r", "termwiz has no kitty encoder");
    }

    #[test]
    fn modify_other_keys_distinguishes_ctrl_enter() {
        let (mut s, w) = session_with_writer(80, 24);
        s.advance_bytes(b"\x1b[>4;2m");
        let mut ctrl_enter = evt("Enter", "Enter");
        ctrl_enter.ctrl = true;
        s.key_down(&ctrl_enter).unwrap();
        assert_eq!(w.drain(), b"\x1b[27;5;13~");
    }

    #[test]
    fn default_mode_keeps_ctrl_enter_legacy_until_modify_other_keys() {
        let (mut s, w) = session_with_writer(80, 24);
        let mut ctrl_enter = evt("Enter", "Enter");
        ctrl_enter.ctrl = true;
        s.key_down(&ctrl_enter).unwrap();
        assert_eq!(w.drain(), b"\r");
    }

    #[test]
    fn primary_device_attributes_are_answered() {
        assert_eq!(answer_to(b"\x1b[c"), b"\x1b[?65;4;6;18;22c");
    }

    #[test]
    fn xtversion_reports_nergal() {
        let expected = format!("\x1bP>|nergal {}\x1b\\", env!("CARGO_PKG_VERSION"));
        assert_eq!(answer_to(b"\x1b[>q"), expected.as_bytes());
    }

    #[test]
    fn background_color_query_reports_the_backend_palette() {
        let answer = String::from_utf8(answer_to(b"\x1b]11;?\x1b\\")).unwrap();
        assert!(answer.starts_with("\x1b]11;rgb:"), "{answer:?}");
    }

    #[test]
    fn synchronized_output_query_reports_recognized_but_reset() {
        assert_eq!(answer_to(b"\x1b[?2026$p"), b"\x1b[?2026;2$y");
    }

    #[test]
    fn focus_change_is_reported_only_after_decset_1004() {
        let (mut s, w) = session_with_writer(80, 24);
        s.focus_changed(false);
        s.focus_changed(true);
        std::thread::sleep(Duration::from_millis(50));
        assert!(w.bytes().is_empty(), "no report without DECSET 1004");

        s.advance_bytes(b"\x1b[?1004h");
        s.focus_changed(false);
        assert_eq!(w.drain(), b"\x1b[O");
        w.clear();
        s.focus_changed(true);
        assert_eq!(w.drain(), b"\x1b[I");
    }

    #[test]
    fn repeated_focus_state_is_not_reported_twice() {
        let (mut s, w) = session_with_writer(80, 24);
        s.advance_bytes(b"\x1b[?1004h");
        s.focus_changed(false);
        s.focus_changed(false);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(w.drain(), b"\x1b[O");
    }

    #[test]
    fn right_and_middle_buttons_report_press_and_release() {
        for (button, code) in [(PrimaryButton::Right, 2), (PrimaryButton::Middle, 1)] {
            let (mut s, w) = session_with_writer(80, 24);
            s.advance_bytes(b"\x1b[?1000h\x1b[?1006h");
            s.mouse_button(button, PrimaryKind::Press, 4, 2, MouseMods::default())
                .unwrap();
            s.mouse_button(button, PrimaryKind::Release, 4, 2, MouseMods::default())
                .unwrap();
            std::thread::sleep(Duration::from_millis(50));
            let expected = format!("\x1b[<{code};5;3M\x1b[<{code};5;3m");
            assert_eq!(w.drain(), expected.as_bytes(), "{button:?}");
        }
    }

    fn plain_text(row: &[CellSnapshot]) -> String {
        row.iter()
            .map(|c| c.ch.as_str())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
}

#[cfg(test)]
#[path = "session_compat_tests.rs"]
mod compat_tests;

#[cfg(test)]
#[path = "mouse_tracking_tests.rs"]
mod mouse_tracking_tests;

#[cfg(test)]
#[path = "mouse_scroll_gate_tests.rs"]
mod mouse_scroll_gate_tests;

#[cfg(test)]
#[path = "image_runs_tests.rs"]
mod image_runs_tests;

#[cfg(test)]
#[path = "session_status_tests.rs"]
mod status_tests;
