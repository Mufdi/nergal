use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use wezterm_term::TerminalConfiguration;
use wezterm_term::color::ColorPalette;
use wezterm_term::{Terminal, TerminalSize};

use super::config::CluihudTerminalConfig;
use super::input::map_event;
use super::types::{CellSnapshot, CursorSnapshot, GridSnapshot, TerminalKeyEvent};

/// A single terminal emulator instance owned by the backend.
///
/// Phase 1 only: no IPC coupling, no coalescing, no grid differ. Those land
/// in Phase 2. What this gives us is the ability to feed PTY bytes in, read
/// the resulting screen state out, and cover it with unit tests.
pub struct TerminalSession {
    terminal: Terminal,
    cols: u16,
    rows: u16,
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
}

impl TerminalSession {
    /// Build a session. `writer` is whatever the terminal should use to talk
    /// back to the PTY (answerbacks, mouse reports, encoded keyboard input).
    /// In tests pass a `Vec<u8>` or `std::io::sink()`; in production this
    /// will be wired to the `portable-pty` master writer.
    pub fn new(cols: u16, rows: u16, writer: Box<dyn Write + Send>) -> Self {
        Self::with_config(cols, rows, writer, CluihudTerminalConfig::new())
    }

    pub fn with_config(
        cols: u16,
        rows: u16,
        writer: Box<dyn Write + Send>,
        config: CluihudTerminalConfig,
    ) -> Self {
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 0,
        };
        let palette = config.color_palette();
        let terminal = Terminal::new(
            size,
            Arc::new(config),
            "cluihud",
            env!("CARGO_PKG_VERSION"),
            writer,
        );
        Self {
            terminal,
            cols,
            rows,
            palette,
            scroll_offset: 0,
            last_scrollback_rows: 0,
        }
    }

    /// Feed bytes from the PTY reader into the VT parser.
    pub fn advance_bytes(&mut self, bytes: &[u8]) {
        self.terminal.advance_bytes(bytes);
    }

    /// Resize the visible area. The PTY resize (ioctl TIOCSWINSZ) stays on
    /// the `pty.rs` side — this only updates the emulator's internal model.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let size = TerminalSize {
            rows: rows as usize,
            cols: cols as usize,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 0,
        };
        self.terminal.resize(size);
        self.cols = cols;
        self.rows = rows;
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

    /// Read-only access to the underlying wezterm terminal. Useful for tests
    /// and for Phase 2 grid-diffing logic that needs richer access than
    /// [`Self::grid_snapshot`] exposes.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
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

    /// True when the underlying terminal is rendering on the alternate
    /// screen (TUI apps like vim, less, claude in fullscreen). Local
    /// scrollback is unavailable there, so wheel events should be
    /// forwarded to the app via [`Self::mouse_wheel`] instead.
    pub fn is_alt_screen_active(&self) -> bool {
        self.terminal.is_alt_screen_active()
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

        for line in &visible {
            let mut row_cells: Vec<CellSnapshot> = Vec::with_capacity(cols);
            for cell in line.visible_cells() {
                let attrs = cell.attrs();
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

        GridSnapshot {
            cols,
            rows,
            cursor,
            title,
            scroll_offset: self.scroll_offset,
        }
    }
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
        s.resize(120, 40);
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
        s.advance_bytes(b"\x1b]0;cluihud test\x07");
        let grid = s.grid_snapshot();
        assert_eq!(grid.title.as_deref(), Some("cluihud test"));
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
    fn shift_enter_distinguishes_when_csi_u_fallback_active() {
        // With Kitty disabled, CSI-u kicks in as the fallback and now
        // Shift+Enter encodes as a distinct CSI-u sequence.
        let w = CapturedWriter::new();
        let cfg = CluihudTerminalConfig::new().with_kitty_keyboard(false);
        let mut s = TerminalSession::with_config(80, 24, Box::new(w.clone()), cfg);

        s.key_down(&evt("Enter", "Enter")).unwrap();
        let plain = w.drain();
        w.clear();

        let mut shift_enter = evt("Enter", "Enter");
        shift_enter.shift = true;
        s.key_down(&shift_enter).unwrap();
        let with_shift = w.drain();

        assert_ne!(
            plain, with_shift,
            "csi_u fallback: Shift+Enter distinct from Enter"
        );
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

    // Note: the old `kitty_disabled_falls_back_to_csi_u` test became
    // redundant once Ctrl+Backspace started remapping unconditionally to
    // Ctrl+W. The CSI-u fallback is now exercised by
    // `shift_enter_distinguishes_when_csi_u_fallback_active`.

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

    fn plain_text(row: &[CellSnapshot]) -> String {
        row.iter()
            .map(|c| c.ch.as_str())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }
}
